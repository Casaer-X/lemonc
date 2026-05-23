use super::coff::CoffFile;
use std::collections::{HashMap, HashSet};

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

struct ImportInfo {
    dll_name: String,
    functions: Vec<String>,
}

struct IdataLayout {
    idt_offset: u32,
    idt_size: u32,
    ilt_offsets: Vec<u32>,
    iat_offsets: Vec<u32>,
    hint_name_offsets: HashMap<String, u32>,
    dll_name_offsets: Vec<u32>,
    iat_start_offset: u32,
    iat_total_size: u32,
    total_size: u32,
    func_iat_offsets: HashMap<String, u32>,
}

fn resolve_dll(func_name: &str) -> String {
    let msvcrt_funcs = [
        "printf", "fprintf", "sprintf", "snprintf", "vsnprintf",
        "malloc", "free", "realloc", "calloc",
        "exit", "abort",
        "strlen", "strcmp", "strncmp", "strdup", "strstr", "memcpy", "memmove", "memset",
        "fopen", "fclose", "fread", "fwrite", "fseek", "ftell", "fgets", "fputs",
        "fputc", "fgetc", "ungetc", "feof",
        "atoi", "atol", "atoll", "atof",
        "longjmp", "_setjmp",
        "__main",
    ];
    if msvcrt_funcs.contains(&func_name) {
        return "msvcrt.dll".to_string();
    }
    "msvcrt.dll".to_string()
}

fn compute_idata_layout(imports: &[ImportInfo]) -> IdataLayout {
    let mut offset = 0u32;

    let idt_offset = 0u32;
    let idt_size = ((imports.len() + 1) * 20) as u32;
    offset += idt_size;

    let mut ilt_offsets = Vec::new();
    for import in imports {
        offset = align_up(offset, 8);
        ilt_offsets.push(offset);
        offset += ((import.functions.len() + 1) * 8) as u32;
    }

    offset = align_up(offset, 8);
    let iat_start_offset = offset;
    let mut iat_offsets = Vec::new();
    let mut func_iat_offsets = HashMap::new();
    for (i, import) in imports.iter().enumerate() {
        offset = align_up(offset, 8);
        iat_offsets.push(offset);
        for (j, func) in import.functions.iter().enumerate() {
            func_iat_offsets.insert(func.clone(), offset + (j as u32) * 8);
            offset += 8;
        }
        offset += 8;
    }
    let iat_total_size = offset - iat_start_offset;

    let mut hint_name_offsets = HashMap::new();
    for import in imports {
        for func in &import.functions {
            offset = align_up(offset, 2);
            hint_name_offsets.insert(func.clone(), offset);
            offset += 2 + func.len() as u32 + 1;
        }
    }

    let mut dll_name_offsets = Vec::new();
    for import in imports {
        offset = align_up(offset, 2);
        dll_name_offsets.push(offset);
        offset += import.dll_name.len() as u32 + 1;
    }

    IdataLayout {
        idt_offset,
        idt_size,
        ilt_offsets,
        iat_offsets,
        hint_name_offsets,
        dll_name_offsets,
        iat_start_offset,
        iat_total_size,
        total_size: offset,
        func_iat_offsets,
    }
}

fn generate_idata_data(imports: &[ImportInfo], layout: &IdataLayout, idata_rva: u32) -> Vec<u8> {
    let mut data = vec![0u8; layout.total_size as usize];

    for (i, _import) in imports.iter().enumerate() {
        let off = layout.idt_offset as usize + i * 20;
        let ilt_rva = idata_rva + layout.ilt_offsets[i];
        let name_rva = idata_rva + layout.dll_name_offsets[i];
        let iat_rva = idata_rva + layout.iat_offsets[i];
        data[off..off + 4].copy_from_slice(&ilt_rva.to_le_bytes());
        data[off + 12..off + 16].copy_from_slice(&name_rva.to_le_bytes());
        data[off + 16..off + 20].copy_from_slice(&iat_rva.to_le_bytes());
    }

    for (i, import) in imports.iter().enumerate() {
        let ilt_base = layout.ilt_offsets[i] as usize;
        let iat_base = layout.iat_offsets[i] as usize;
        for (j, func) in import.functions.iter().enumerate() {
            let hint_name_rva = idata_rva + layout.hint_name_offsets[func];
            let ilt_off = ilt_base + j * 8;
            let iat_off = iat_base + j * 8;
            data[ilt_off..ilt_off + 8].copy_from_slice(&(hint_name_rva as u64).to_le_bytes());
            data[iat_off..iat_off + 8].copy_from_slice(&(hint_name_rva as u64).to_le_bytes());
        }
    }

    for import in imports {
        for func in &import.functions {
            let off = layout.hint_name_offsets[func] as usize;
            data[off] = 0;
            data[off + 1] = 0;
            let name_bytes = func.as_bytes();
            data[off + 2..off + 2 + name_bytes.len()].copy_from_slice(name_bytes);
            data[off + 2 + name_bytes.len()] = 0;
        }
    }

    for (i, import) in imports.iter().enumerate() {
        let off = layout.dll_name_offsets[i] as usize;
        let name_bytes = import.dll_name.as_bytes();
        data[off..off + name_bytes.len()].copy_from_slice(name_bytes);
        data[off + name_bytes.len()] = 0;
    }

    data
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

    /// Add embedded .lmb bytecode data as a custom PE section (.lmb).
    ///
    /// The data will be placed in a read-only section with two exported symbols:
    /// - `_lmb_data_start` → points to the beginning of the bytecode
    /// - `_lmb_data_size`  → the size in bytes (as a 32-bit value in .rdata)
    ///
    /// This allows native code to reference the embedded bytecode at runtime:
    /// ```c
    /// extern const uint8_t _lmb_data_start[];
    /// extern const uint32_t _lmb_data_size;
    /// LeVM* vm = le_vm_create(_lmb_data_start, _lmb_data_size);
    /// ```
    pub fn add_embedded_lmb(&mut self, lmb_data: &[u8]) {
        // Add the .lmb section with the bytecode data
        // Characteristics: IMAGE_SCN_CNT_INITIALIZED_DATA | IMAGE_SCN_MEM_READ
        // = 0x00000040 | 0x40000000 = 0x40000040
        self.merged_sections.push(MergedSection {
            name: ".lmb".to_string(),
            characteristics: 0x40000040, // CNT_INITIALIZED_DATA | MEM_READ
            data: lmb_data.to_vec(),
            symbols: vec![("_lmb_data_start".to_string(), 0)],
            relocs: Vec::new(),
        });

        // Add the size constant in .rdata
        let rdata = if let Some(pos) = self.merged_sections.iter().position(|s| s.name == ".rdata") {
            &mut self.merged_sections[pos]
        } else {
            self.merged_sections.push(MergedSection {
                name: ".rdata".to_string(),
                characteristics: 0x40000040,
                data: Vec::new(),
                symbols: Vec::new(),
                relocs: Vec::new(),
            });
            self.merged_sections.last_mut().unwrap()
        };

        let size_offset = align_up(rdata.data.len() as u32, 4);
        if size_offset > rdata.data.len() as u32 {
            rdata.data.extend(std::iter::repeat(0u8).take((size_offset - rdata.data.len() as u32) as usize));
        }
        let size_offset = rdata.data.len() as u32;
        rdata.data.extend_from_slice(&(lmb_data.len() as u32).to_le_bytes());
        rdata.symbols.push(("_lmb_data_size".to_string(), size_offset));
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
            (".lmb", ".lmb"),
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
                    let sym_idx = coff.raw_to_symbol_idx
                        .get(&reloc.symbol_index)
                        .copied()
                        .unwrap_or(reloc.symbol_index as usize);
                    if sym_idx >= coff.symbols.len() {
                        continue;
                    }
                    let sym = &coff.symbols[sym_idx];
                    let new_va = base_offset + reloc.virtual_address;
                    merged.relocs.push((new_va, sym.name.clone(), reloc.typ));
                }
            }
        }

        Ok(())
    }

    pub fn build_pe(&mut self) -> Result<Vec<u8>, String> {
        if self.merged_sections.is_empty() {
            return Err("No sections to link".to_string());
        }

        let defined_symbols: HashSet<String> = self.merged_sections.iter()
            .flat_map(|s| s.symbols.iter().map(|(name, _)| name.clone()))
            .collect();

        let mut undefined: Vec<String> = Vec::new();
        let mut seen_undefined: HashSet<String> = HashSet::new();
        for section in &self.merged_sections {
            for (_, sym_name, _) in &section.relocs {
                if !defined_symbols.contains(sym_name) && !seen_undefined.contains(sym_name) {
                    undefined.push(sym_name.clone());
                    seen_undefined.insert(sym_name.clone());
                }
            }
        }

        let builtin_stubs = ["__main"];
        let mut stub_map: HashMap<String, u32> = HashMap::new();
        if let Some(text_section) = self.merged_sections.iter_mut().find(|s| s.name == ".text") {
            let mut stub_offset = align_up(text_section.data.len() as u32, 16);
            if stub_offset > text_section.data.len() as u32 {
                text_section.data.extend(std::iter::repeat(0u8).take((stub_offset - text_section.data.len() as u32) as usize));
            }
            for stub_name in &builtin_stubs {
                if undefined.contains(&stub_name.to_string()) {
                    stub_map.insert(stub_name.to_string(), stub_offset);
                    text_section.data.push(0xC3);
                    stub_offset += 1;
                }
            }
        }
        undefined.retain(|f| !builtin_stubs.contains(&f.as_str()));

        let mut imports: Vec<ImportInfo> = Vec::new();
        for func in &undefined {
            let dll = resolve_dll(func);
            if let Some(import) = imports.iter_mut().find(|i| i.dll_name == dll) {
                if !import.functions.contains(func) {
                    import.functions.push(func.clone());
                }
            } else {
                imports.push(ImportInfo {
                    dll_name: dll,
                    functions: vec![func.clone()],
                });
            }
        }

        let mut thunk_map: HashMap<String, u32> = HashMap::new();
        let mut thunk_patches: Vec<(u32, String)> = Vec::new();

        if !undefined.is_empty() {
            let text_section = self.merged_sections.iter_mut().find(|s| s.name == ".text")
                .ok_or_else(|| "No .text section for import thunks".to_string())?;

            let mut thunk_offset = align_up(text_section.data.len() as u32, 16);
            if thunk_offset > text_section.data.len() as u32 {
                text_section.data.extend(std::iter::repeat(0u8).take((thunk_offset - text_section.data.len() as u32) as usize));
            }

            for func in &undefined {
                thunk_map.insert(func.clone(), thunk_offset);
                text_section.data.extend_from_slice(&[0xFF, 0x25, 0x00, 0x00, 0x00, 0x00]);
                thunk_patches.push((thunk_offset + 2, func.clone()));
                thunk_offset += 6;
            }
        }

        let idata_layout = if !imports.is_empty() {
            let layout = compute_idata_layout(&imports);
            self.merged_sections.push(MergedSection {
                name: ".idata".to_string(),
                characteristics: 0xC0000040,
                data: vec![0u8; layout.total_size as usize],
                symbols: Vec::new(),
                relocs: Vec::new(),
            });
            Some(layout)
        } else {
            None
        };

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

        let (import_dir_rva, import_dir_size, iat_rva, iat_size) = if let Some(ref layout) = idata_layout {
            let idata_idx = self.merged_sections.iter().position(|s| s.name == ".idata").unwrap();
            let idata_rva = section_rvas[idata_idx];
            let idata_data = generate_idata_data(&imports, layout, idata_rva);
            self.merged_sections[idata_idx].data = idata_data;
            (
                idata_rva + layout.idt_offset,
                layout.idt_size,
                idata_rva + layout.iat_start_offset,
                layout.iat_total_size,
            )
        } else {
            (0, 0, 0, 0)
        };

        let mut global_symbols: HashMap<String, (usize, u32)> = HashMap::new();
        for (sec_idx, section) in self.merged_sections.iter().enumerate() {
            for (name, offset) in &section.symbols {
                let rva = section_rvas[sec_idx] + offset;
                global_symbols.insert(name.clone(), (sec_idx, rva));
            }
        }

        if !thunk_map.is_empty() {
            let text_sec_idx = self.merged_sections.iter().position(|s| s.name == ".text").unwrap();
            for (func_name, thunk_offset) in &thunk_map {
                let rva = section_rvas[text_sec_idx] + thunk_offset;
                global_symbols.insert(func_name.clone(), (text_sec_idx, rva));
            }
        }

        if !stub_map.is_empty() {
            let text_sec_idx = self.merged_sections.iter().position(|s| s.name == ".text").unwrap();
            for (func_name, stub_offset) in &stub_map {
                let rva = section_rvas[text_sec_idx] + stub_offset;
                global_symbols.insert(func_name.clone(), (text_sec_idx, rva));
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
        self.write_optional_header(&mut buf, size_of_headers, size_of_image, entry_rva, base_of_code, import_dir_rva, import_dir_size, iat_rva, iat_size);
        self.write_section_headers(&mut buf, num_sections, &section_rvas, &section_raw_offsets, &section_sizes);
        self.pad_to(&mut buf, size_of_headers as usize);
        self.write_section_data(&mut buf, &section_sizes);
        self.apply_relocations(&mut buf, &section_rvas, &section_raw_offsets, &global_symbols)?;

        if let Some(ref layout) = idata_layout {
            let idata_idx = self.merged_sections.iter().position(|s| s.name == ".idata").unwrap();
            let idata_rva = section_rvas[idata_idx];
            let text_sec_idx = self.merged_sections.iter().position(|s| s.name == ".text").unwrap();
            let text_raw = section_raw_offsets[text_sec_idx];
            let text_rva = section_rvas[text_sec_idx];

            for (disp32_offset_in_text, func_name) in &thunk_patches {
                let iat_offset_in_idata = layout.func_iat_offsets.get(func_name)
                    .ok_or_else(|| format!("No IAT entry for imported function '{}'", func_name))?;
                let iat_entry_rva = idata_rva + iat_offset_in_idata;

                let patch_file_offset = (text_raw + disp32_offset_in_text) as usize;
                let thunk_rva = text_rva + (disp32_offset_in_text - 2);
                let next_rip = thunk_rva + 6;
                let disp32 = (iat_entry_rva as i64 - next_rip as i64) as i32;

                if patch_file_offset + 4 > buf.len() {
                    return Err(format!("Thunk patch at offset {} out of bounds", patch_file_offset));
                }
                buf[patch_file_offset..patch_file_offset + 4].copy_from_slice(&disp32.to_le_bytes());
            }
        }

        Ok(buf)
    }

    fn write_dos_header(&self, buf: &mut Vec<u8>) {
        write_u16(buf, 0x5A4D);
        for _ in 2..60 {
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
        import_dir_rva: u32,
        import_dir_size: u32,
        iat_rva: u32,
        iat_size: u32,
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
        write_u16(buf, 6);
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

        for i in 0..16u32 {
            match i {
                1 => {
                    write_u32(buf, import_dir_rva);
                    write_u32(buf, import_dir_size);
                }
                12 => {
                    write_u32(buf, iat_rva);
                    write_u32(buf, iat_size);
                }
                _ => {
                    write_u32(buf, 0);
                    write_u32(buf, 0);
                }
            }
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
