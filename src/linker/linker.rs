use super::coff::CoffFile;
use std::collections::HashMap;

const IMAGE_REL_AMD64_ADDR32NB: u16 = 2;
const IMAGE_REL_AMD64_REL32: u16 = 4;

const DOS_HEADER_SIZE: usize = 64;
const PE_SIGNATURE_SIZE: usize = 4;
const COFF_HEADER_SIZE: usize = 24;
const OPTIONAL_HEADER_SIZE: usize = 240;
const SECTION_HEADER_SIZE: usize = 40;

const FILE_ALIGNMENT: u32 = 0x200;
const SECTION_ALIGNMENT: u32 = 0x1000;
const IMAGE_BASE: u64 = 0x400000;

struct MergedSection {
    name: String,
    characteristics: u32,
    data: Vec<u8>,
    symbols: Vec<(String, u32)>,
    relocs: Vec<(u32, String, u16)>,
}

pub struct PeLinker {
    merged_sections: Vec<MergedSection>,
    entry_point: String,
}

impl PeLinker {
    pub fn new() -> Self {
        Self {
            merged_sections: Vec::new(),
            entry_point: "main".to_string(),
        }
    }

    pub fn set_entry_point(&mut self, name: &str) {
        self.entry_point = name.to_string();
    }

    pub fn add_object(&mut self, data: &[u8]) -> Result<(), String> {
        let coff = CoffFile::parse(data)?;
        self.merge_coff(&coff)
    }

    fn merge_coff(&mut self, coff: &CoffFile) -> Result<(), String> {
        let section_name_map: HashMap<&str, &str> = [
            (".text", ".text"),
            (".data", ".data"),
            (".rdata", ".rdata"),
            (".bss", ".bss"),
        ]
        .iter()
        .cloned()
        .collect();

        for (sec_idx, section) in coff.sections.iter().enumerate() {
            let target_name = section_name_map
                .get(section.name.as_str())
                .copied()
                .unwrap_or(&section.name);

            let merged = if let Some(pos) = self
                .merged_sections
                .iter()
                .position(|s| s.name == target_name)
            {
                &mut self.merged_sections[pos]
            } else {
                self.merged_sections.push(MergedSection {
                    name: target_name.to_string(),
                    characteristics: section.characteristics,
                    data: Vec::new(),
                    symbols: Vec::new(),
                    relocs: Vec::new(),
                });
                self.merged_sections.last_mut().unwrap()
            };

            let base_offset = merged.data.len() as u32;
            merged.data.extend_from_slice(&section.data);

            let align = 16u32;
            let padded = align_up(section.data.len() as u32, align);
            if padded > section.data.len() as u32 {
                merged.data.extend(std::iter::repeat(0u8).take((padded - section.data.len() as u32) as usize));
            }

            merged.characteristics |= section.characteristics;

            for (sym_idx, symbol) in coff.symbols.iter().enumerate() {
                if symbol.section_number as usize == sec_idx + 1
                    && symbol.storage_class == 2
                    && symbol.section_number > 0
                {
                    let new_offset = base_offset + symbol.value;
                    merged.symbols.push((symbol.name.clone(), new_offset));
                }
            }

            if let Some(relocs) = coff.relocations.get(&(sec_idx as u16)) {
                for reloc in relocs {
                    let sym = &coff.symbols[reloc.symbol_index as usize];
                    let new_va = base_offset + reloc.virtual_address;
                    merged.relocs.push((new_va, sym.name.clone(), reloc.typ));
                }
            }
        }

        Ok(())
    }

    pub fn build_pe(&self) -> Result<Vec<u8>, String> {
        if self.merged_sections.is_empty() {
            return Err("No sections to link".to_string());
        }

        let num_sections = self.merged_sections.len() as u16;

        let headers_raw_size = DOS_HEADER_SIZE
            + PE_SIGNATURE_SIZE
            + COFF_HEADER_SIZE
            + OPTIONAL_HEADER_SIZE
            + (SECTION_HEADER_SIZE * num_sections as usize);
        let size_of_headers = align_up(headers_raw_size as u32, FILE_ALIGNMENT);

        let mut section_rvas: Vec<u32> = Vec::new();
        let mut section_raw_offsets: Vec<u32> = Vec::new();
        let mut section_sizes: Vec<u32> = Vec::new();

        let mut current_rva = align_up(size_of_headers, SECTION_ALIGNMENT);
        let mut current_raw = size_of_headers;

        for section in &self.merged_sections {
            section_rvas.push(current_rva);
            section_raw_offsets.push(current_raw);

            let raw_size = align_up(section.data.len() as u32, FILE_ALIGNMENT);
            section_sizes.push(raw_size);

            current_rva = align_up(current_rva + align_up(section.data.len() as u32, SECTION_ALIGNMENT), SECTION_ALIGNMENT);
            current_raw += raw_size;
        }

        let size_of_image = align_up(current_rva, SECTION_ALIGNMENT);

        let mut global_symbols: HashMap<String, (usize, u32)> = HashMap::new();
        for (sec_idx, section) in self.merged_sections.iter().enumerate() {
            for (name, offset) in &section.symbols {
                let rva = section_rvas[sec_idx] + offset;
                global_symbols.insert(name.clone(), (sec_idx, rva));
            }
        }

        let entry_rva = global_symbols
            .get(&self.entry_point)
            .map(|(_, rva)| *rva)
            .ok_or_else(|| format!("Entry point '{}' not found", self.entry_point))?;

        let base_of_code = if !section_rvas.is_empty() { section_rvas[0] } else { 0 };

        let mut buf: Vec<u8> = Vec::new();

        self.write_dos_header(&mut buf);
        self.write_pe_signature(&mut buf);
        self.write_coff_header(&mut buf, num_sections);
        self.write_optional_header(&mut buf, size_of_headers, size_of_image, entry_rva, base_of_code);
        self.write_section_headers(&mut buf, num_sections, &section_rvas, &section_raw_offsets, &section_sizes);
        self.pad_to(&mut buf, size_of_headers as usize);
        self.write_section_data(&mut buf, &section_sizes);
        self.apply_relocations(&mut buf, &section_rvas, &section_raw_offsets, &global_symbols)?;

        Ok(buf)
    }

    fn write_dos_header(&self, buf: &mut Vec<u8>) {
        write_u16(buf, 0x5A4D);
        for _ in 2..58 {
            buf.push(0);
        }
        write_u32(buf, DOS_HEADER_SIZE as u32);
    }

    fn write_pe_signature(&self, buf: &mut Vec<u8>) {
        buf.extend_from_slice(b"PE\0\0");
    }

    fn write_coff_header(&self, buf: &mut Vec<u8>, num_sections: u16) {
        write_u16(buf, 0x8664);
        write_u16(buf, num_sections);
        write_u32(buf, 0);
        write_u32(buf, 0);
        write_u32(buf, 0);
        write_u16(buf, OPTIONAL_HEADER_SIZE as u16);
        write_u16(buf, 0x0022);
    }

    fn write_optional_header(
        &self,
        buf: &mut Vec<u8>,
        size_of_headers: u32,
        size_of_image: u32,
        entry_rva: u32,
        base_of_code: u32,
    ) {
        write_u16(buf, 0x020B);
        write_u8(buf, 14);
        write_u8(buf, 0);
        write_u32(buf, 0);
        write_u32(buf, 0);
        write_u32(buf, 0);
        write_u32(buf, entry_rva);
        write_u32(buf, base_of_code);

        write_u64(buf, IMAGE_BASE);
        write_u32(buf, SECTION_ALIGNMENT);
        write_u32(buf, FILE_ALIGNMENT);
        write_u16(buf, 6);
        write_u16(buf, 0);
        write_u16(buf, 0);
        write_u16(buf, 0);
        write_u16(buf, 0);
        write_u16(buf, 0);
        write_u32(buf, 0);
        write_u32(buf, size_of_image);
        write_u32(buf, size_of_headers);
        write_u32(buf, 0);
        write_u16(buf, 3);
        write_u16(buf, 0);
        write_u64(buf, 0x100000);
        write_u64(buf, 0x1000);
        write_u64(buf, 0x100000);
        write_u64(buf, 0x1000);
        write_u32(buf, 0);
        write_u32(buf, 16);

        for _ in 0..16 {
            write_u32(buf, 0);
            write_u32(buf, 0);
        }
    }

    fn write_section_headers(
        &self,
        buf: &mut Vec<u8>,
        num_sections: u16,
        section_rvas: &[u32],
        section_raw_offsets: &[u32],
        section_sizes: &[u32],
    ) {
        for i in 0..num_sections as usize {
            let section = &self.merged_sections[i];
            write_name(buf, &section.name, 8);

            let virtual_size = section.data.len() as u32;
            write_u32(buf, virtual_size);
            write_u32(buf, section_rvas[i]);
            write_u32(buf, section_sizes[i]);
            write_u32(buf, section_raw_offsets[i]);
            write_u32(buf, 0);
            write_u32(buf, 0);
            write_u16(buf, 0);
            write_u16(buf, 0);
            write_u32(buf, section.characteristics);
        }
    }

    fn pad_to(&self, buf: &mut Vec<u8>, target_size: usize) {
        while buf.len() < target_size {
            buf.push(0);
        }
    }

    fn write_section_data(&self, buf: &mut Vec<u8>, section_sizes: &[u32]) {
        for (i, section) in self.merged_sections.iter().enumerate() {
            let start = buf.len();
            buf.extend_from_slice(&section.data);
            let target = start + section_sizes[i] as usize;
            while buf.len() < target {
                buf.push(0);
            }
        }
    }

    fn apply_relocations(
        &self,
        buf: &mut Vec<u8>,
        section_rvas: &[u32],
        section_raw_offsets: &[u32],
        global_symbols: &HashMap<String, (usize, u32)>,
    ) -> Result<(), String> {
        for (sec_idx, section) in self.merged_sections.iter().enumerate() {
            let sec_rva = section_rvas[sec_idx];
            let sec_raw = section_raw_offsets[sec_idx];

            for (offset, sym_name, reloc_type) in &section.relocs {
                let (sym_sec_idx, sym_rva) = global_symbols
                    .get(sym_name)
                    .ok_or_else(|| format!("Undefined symbol: {}", sym_name))?;

                let patch_file_offset = (sec_raw + offset) as usize;
                let patch_rva = sec_rva + offset;

                match *reloc_type {
                    IMAGE_REL_AMD64_ADDR32NB => {
                        if patch_file_offset + 4 > buf.len() {
                            return Err(format!(
                                "ADDR32NB relocation at offset {} out of bounds",
                                patch_file_offset
                            ));
                        }
                        let target_rva = *sym_rva;
                        let bytes = (target_rva as u32).to_le_bytes();
                        buf[patch_file_offset..patch_file_offset + 4].copy_from_slice(&bytes);
                    }
                    IMAGE_REL_AMD64_REL32 => {
                        if patch_file_offset + 4 > buf.len() {
                            return Err(format!(
                                "REL32 relocation at offset {} out of bounds",
                                patch_file_offset
                            ));
                        }
                        let target_rva = *sym_rva;
                        let rel32 = target_rva as i32 - (patch_rva as i32 + 4);
                        let bytes = rel32.to_le_bytes();
                        buf[patch_file_offset..patch_file_offset + 4].copy_from_slice(&bytes);
                    }
                    _ => {
                        return Err(format!("Unsupported relocation type: {}", reloc_type));
                    }
                }
            }
        }

        Ok(())
    }
}

fn write_u8(buf: &mut Vec<u8>, val: u8) {
    buf.push(val);
}

fn write_u16(buf: &mut Vec<u8>, val: u16) {
    buf.extend_from_slice(&val.to_le_bytes());
}

fn write_u32(buf: &mut Vec<u8>, val: u32) {
    buf.extend_from_slice(&val.to_le_bytes());
}

fn write_u64(buf: &mut Vec<u8>, val: u64) {
    buf.extend_from_slice(&val.to_le_bytes());
}

fn write_name(buf: &mut Vec<u8>, name: &str, max_len: usize) {
    let bytes = name.as_bytes();
    let copy_len = bytes.len().min(max_len);
    buf.extend_from_slice(&bytes[..copy_len]);
    for _ in copy_len..max_len {
        buf.push(0);
    }
}

fn align_up(val: u32, align: u32) -> u32 {
    (val + align - 1) & !(align - 1)
}

pub fn link_pe(coff_data: &[u8]) -> Result<Vec<u8>, String> {
    let mut linker = PeLinker::new();
    linker.add_object(coff_data)?;
    linker.build_pe()
}
