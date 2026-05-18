use std::collections::HashMap;

pub struct CoffFile {
    pub sections: Vec<CoffSection>,
    pub symbols: Vec<CoffSymbol>,
    pub string_table: Vec<u8>,
    pub relocations: HashMap<u16, Vec<CoffReloc>>,
}

pub struct CoffSection {
    pub name: String,
    pub virtual_size: u32,
    pub virtual_address: u32,
    pub raw_data_size: u32,
    pub raw_data_offset: u32,
    pub reloc_offset: u32,
    pub num_relocs: u16,
    pub characteristics: u32,
    pub data: Vec<u8>,
}

pub struct CoffSymbol {
    pub name: String,
    pub value: u32,
    pub section_number: i16,
    pub typ: u16,
    pub storage_class: u8,
    pub num_aux: u8,
}

pub struct CoffReloc {
    pub virtual_address: u32,
    pub symbol_index: u32,
    pub typ: u16,
}

impl CoffFile {
    pub fn parse(data: &[u8]) -> Result<Self, String> {
        if data.len() < 20 {
            return Err("COFF header too short".to_string());
        }

        let machine = u16::from_le_bytes([data[0], data[1]]);
        if machine != 0x8664 {
            return Err(format!("Not a COFF64 file (machine=0x{:04x})", machine));
        }

        let num_sections = u16::from_le_bytes([data[2], data[3]]);
        let _timestamp = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
        let _sym_table_ptr = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);
        let num_symbols = u32::from_le_bytes([data[12], data[13], data[14], data[15]]);
        let _opt_header_size = u16::from_le_bytes([data[16], data[17]]);
        let _characteristics = u16::from_le_bytes([data[18], data[19]]);

        let mut offset = 20u32;
        let mut sections = Vec::new();

        for i in 0..num_sections {
            if offset as usize + 40 > data.len() {
                return Err("Section header truncated".to_string());
            }

            let sec_data = &data[offset as usize..];
            let name_bytes = &sec_data[0..8];
            let name = if name_bytes[0] == b'/' {
                let str_off_str = std::str::from_utf8(&name_bytes[1..8])
                    .unwrap_or("0")
                    .trim_end_matches('\0');
                let str_off: usize = str_off_str.parse().unwrap_or(0);
                format!("/{}", str_off)
            } else {
                let end = name_bytes.iter().position(|&b| b == 0).unwrap_or(8);
                String::from_utf8_lossy(&name_bytes[..end]).to_string()
            };

            let virtual_size = u32::from_le_bytes([sec_data[8], sec_data[9], sec_data[10], sec_data[11]]);
            let virtual_address = u32::from_le_bytes([sec_data[12], sec_data[13], sec_data[14], sec_data[15]]);
            let raw_data_size = u32::from_le_bytes([sec_data[16], sec_data[17], sec_data[18], sec_data[19]]);
            let raw_data_offset = u32::from_le_bytes([sec_data[20], sec_data[21], sec_data[22], sec_data[23]]);
            let reloc_offset = u32::from_le_bytes([sec_data[24], sec_data[25], sec_data[26], sec_data[27]]);
            let num_relocs = u16::from_le_bytes([sec_data[32], sec_data[33]]);
            let characteristics = u32::from_le_bytes([sec_data[36], sec_data[37], sec_data[38], sec_data[39]]);

            let mut sec_data_vec = Vec::new();
            if raw_data_size > 0 && raw_data_offset as usize + raw_data_size as usize <= data.len() {
                sec_data_vec.extend_from_slice(&data[raw_data_offset as usize..raw_data_offset as usize + raw_data_size as usize]);
            }

            sections.push(CoffSection {
                name,
                virtual_size,
                virtual_address,
                raw_data_size,
                raw_data_offset,
                reloc_offset,
                num_relocs,
                characteristics,
                data: sec_data_vec,
            });

            offset += 40;
        }

        let sym_table_ptr = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);
        let mut symbols = Vec::new();
        let mut sym_offset = sym_table_ptr;

        let string_table_start = sym_table_ptr + num_symbols * 18;
        let string_table = if string_table_start as usize + 4 <= data.len() {
            let st_size = u32::from_le_bytes([
                data[string_table_start as usize],
                data[string_table_start as usize + 1],
                data[string_table_start as usize + 2],
                data[string_table_start as usize + 3],
            ]);
            let end = (string_table_start as usize) + st_size as usize;
            if end <= data.len() {
                data[string_table_start as usize..end].to_vec()
            } else {
                vec![0, 0, 0, 4]
            }
        } else {
            vec![0, 0, 0, 4]
        };

        for _ in 0..num_symbols {
            if sym_offset as usize + 18 > data.len() {
                break;
            }

            let sym_data = &data[sym_offset as usize..];
            let name_bytes = &sym_data[0..8];
            let name = if name_bytes[0..4] == [0, 0, 0, 0] {
                let str_off = u32::from_le_bytes([name_bytes[4], name_bytes[5], name_bytes[6], name_bytes[7]]);
                if (str_off as usize) < string_table.len() {
                    let st = &string_table[str_off as usize..];
                    let end = st.iter().position(|&b| b == 0).unwrap_or(st.len());
                    String::from_utf8_lossy(&st[..end]).to_string()
                } else {
                    format!("?str_off_{}", str_off)
                }
            } else {
                let end = name_bytes.iter().position(|&b| b == 0).unwrap_or(8);
                String::from_utf8_lossy(&name_bytes[..end]).to_string()
            };

            let value = u32::from_le_bytes([sym_data[8], sym_data[9], sym_data[10], sym_data[11]]);
            let section_number = i16::from_le_bytes([sym_data[12], sym_data[13]]);
            let typ = u16::from_le_bytes([sym_data[14], sym_data[15]]);
            let storage_class = sym_data[16];
            let num_aux = sym_data[17];

            symbols.push(CoffSymbol {
                name,
                value,
                section_number,
                typ,
                storage_class,
                num_aux,
            });

            sym_offset += 18;
            for _ in 0..num_aux {
                sym_offset += 18;
            }
        }

        let mut relocations: HashMap<u16, Vec<CoffReloc>> = HashMap::new();
        for (i, sec) in sections.iter().enumerate() {
            if sec.num_relocs == 0 || sec.reloc_offset == 0 {
                continue;
            }

            let mut relocs = Vec::new();
            let mut reloc_off = sec.reloc_offset;
            for _ in 0..sec.num_relocs {
                if reloc_off as usize + 10 > data.len() {
                    break;
                }
                let rdata = &data[reloc_off as usize..];
                let va = u32::from_le_bytes([rdata[0], rdata[1], rdata[2], rdata[3]]);
                let sym_idx = u32::from_le_bytes([rdata[4], rdata[5], rdata[6], rdata[7]]);
                let rtype = u16::from_le_bytes([rdata[8], rdata[9]]);
                relocs.push(CoffReloc {
                    virtual_address: va,
                    symbol_index: sym_idx,
                    typ: rtype,
                });
                reloc_off += 10;
            }
            relocations.insert(i as u16, relocs);
        }

        Ok(CoffFile {
            sections,
            symbols,
            string_table,
            relocations,
        })
    }

    pub fn find_symbol(&self, name: &str) -> Option<(usize, &CoffSymbol)> {
        self.symbols.iter().enumerate().find(|(_, s)| s.name == name)
    }

    pub fn section_by_name(&self, name: &str) -> Option<(usize, &CoffSection)> {
        self.sections.iter().enumerate().find(|(_, s)| s.name == name)
    }
}
