use crate::ast::node::*;
use std::collections::{HashMap, HashSet};
use std::io::Write;

pub struct CCodeGen {
    output: Vec<u8>,
    indent: u32,
    string_literals: Vec<String>,
    string_counter: u32,
    current_class: Option<String>,
    parent_class: Option<String>,
    var_types: HashMap<String, String>,
    class_fields: HashMap<String, Vec<(String, String)>>,
    virtual_methods: HashMap<String, Vec<VirtualMethodEntry>>,
    interface_methods: HashMap<String, Vec<InterfaceMethodEntry>>,
    class_implements: HashMap<String, Vec<String>>,
    interface_names: HashSet<String>,
    var_interface_types: HashMap<String, String>,
    exc_counter: u32,
    generic_instances: HashMap<String, Vec<Vec<TypeRef>>>,
    generic_classes: HashMap<String, ClassDecl>,
    method_signatures: HashMap<String, Vec<(String, Vec<TypeRef>)>>,
    enums: HashMap<String, EnumDecl>,
    match_counter: u32,
}

struct VirtualMethodEntry {
    name: String,
    return_type: TypeRef,
    params: Vec<Param>,
}

struct InterfaceMethodEntry {
    name: String,
    return_type: TypeRef,
    params: Vec<Param>,
}

impl Clone for InterfaceMethodEntry {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            return_type: self.return_type.clone(),
            params: self.params.clone(),
        }
    }
}

impl Clone for VirtualMethodEntry {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            return_type: self.return_type.clone(),
            params: self.params.clone(),
        }
    }
}

impl CCodeGen {
    pub fn new() -> Self {
        Self {
            output: Vec::new(),
            indent: 0,
            string_literals: Vec::new(),
            string_counter: 0,
            current_class: None,
            parent_class: None,
            var_types: HashMap::new(),
            class_fields: HashMap::new(),
            virtual_methods: HashMap::new(),
            interface_methods: HashMap::new(),
            class_implements: HashMap::new(),
            interface_names: HashSet::new(),
            var_interface_types: HashMap::new(),
            exc_counter: 0,
            generic_instances: HashMap::new(),
            generic_classes: HashMap::new(),
            method_signatures: HashMap::new(),
            enums: HashMap::new(),
            match_counter: 0,
        }
    }

    pub fn generate(&mut self, ast: &Program) -> String {
        for decl in &ast.declarations {
            if let Declaration::Class(class) = decl {
                let mut fields = Vec::new();
                self.collect_class_fields(class, &mut fields);
                self.class_fields.insert(class.name.clone(), fields);
                if !class.type_params.is_empty() {
                    self.generic_classes.insert(class.name.clone(), class.clone());
                }
            }
        }

        self.collect_virtual_methods(ast);
        self.collect_interface_info(ast);
        self.collect_generic_instances(ast);
        self.collect_method_signatures(ast);

        self.emit_line("#include <stdio.h>");
        self.emit_line("#include <stdlib.h>");
        self.emit_line("#include <string.h>");
        self.emit_line("#include <stdint.h>");
        self.emit_line("#include <inttypes.h>");
        self.emit_line("#include <stdarg.h>");
        self.emit_line("#include <setjmp.h>");
        self.emit_line("");

        self.emit_line("typedef struct LemonException {");
        self.emit_line("    const char* type;");
        self.emit_line("    void* value;");
        self.emit_line("    jmp_buf buf;");
        self.emit_line("} LemonException;");
        self.emit_line("");
        self.emit_line("static LemonException* _exc_stack = NULL;");
        self.emit_line("");
        self.emit_line("void lemon_throw(const char* type, void* value) {");
        self.emit_line("    if (!_exc_stack) { fprintf(stderr, \"Unhandled exception: %s\\n\", type); exit(1); }");
        self.emit_line("    _exc_stack->type = type;");
        self.emit_line("    _exc_stack->value = value;");
        self.emit_line("    longjmp(_exc_stack->buf, 1);");
        self.emit_line("}");
        self.emit_line("");

        self.emit_line("typedef void* LemonObject;");
        self.emit_line("");

        self.emit_line("typedef struct TypeInfo {");
        self.emit_line("    char* name;");
        self.emit_line("    int size;");
        self.emit_line("    void (*dtor)(void*);");
        self.emit_line("} TypeInfo;");
        self.emit_line("");

        self.emit_line("typedef struct GcHeader {");
        self.emit_line("    TypeInfo* type_info;");
        self.emit_line("    int marked;");
        self.emit_line("    struct GcHeader* next;");
        self.emit_line("} GcHeader;");
        self.emit_line("");

        self.emit_line("static GcHeader* _gc_root = NULL;");
        self.emit_line("static int _gc_enabled = 1;");
        self.emit_line("");

        self.emit_line("void gc_init() { _gc_root = NULL; _gc_enabled = 1; }");
        self.emit_line("");
        self.emit_line("void* gc_alloc(size_t size, TypeInfo* ti) {");
        self.emit_line("    if (!_gc_enabled) return malloc(size);");
        self.emit_line("    GcHeader* hdr = (GcHeader*)malloc(sizeof(GcHeader) + size);");
        self.emit_line("    if (!hdr) return NULL;");
        self.emit_line("    hdr->type_info = ti;");
        self.emit_line("    hdr->marked = 0;");
        self.emit_line("    hdr->next = _gc_root;");
        self.emit_line("    _gc_root = hdr;");
        self.emit_line("    return (void*)(hdr + 1);");
        self.emit_line("}");
        self.emit_line("");
        self.emit_line("void gc_mark(void* obj) {");
        self.emit_line("    if (!obj) return;");
        self.emit_line("    GcHeader* hdr = ((GcHeader*)obj) - 1;");
        self.emit_line("    hdr->marked = 1;");
        self.emit_line("}");
        self.emit_line("void gc_unmark_all() {");
        self.emit_line("    GcHeader* cur = _gc_root;");
        self.emit_line("    while (cur) { cur->marked = 0; cur = cur->next; }");
        self.emit_line("}");
        self.emit_line("void gc_sweep() {");
        self.emit_line("    GcHeader** p = &_gc_root;");
        self.emit_line("    while (*p) {");
        self.emit_line("        if (!(*p)->marked) {");
        self.emit_line("            GcHeader* unreached = *p;");
        self.emit_line("            *p = unreached->next;");
        self.emit_line("            if (unreached->type_info && unreached->type_info->dtor) {");
        self.emit_line("                unreached->type_info->dtor((void*)(unreached + 1));");
        self.emit_line("            }");
        self.emit_line("            free(unreached);");
        self.emit_line("        } else {");
        self.emit_line("            (*p)->marked = 0;");
        self.emit_line("            p = &(*p)->next;");
        self.emit_line("        }");
        self.emit_line("    }");
        self.emit_line("}");
        self.emit_line("");

        self.emit_line("static TypeInfo _type_info_unknown = {\"unknown\", 0, NULL};");
        self.emit_line("TypeInfo* type_of(void* obj) {");
        self.emit_line("    (void)obj;");
        self.emit_line("    return &_type_info_unknown;");
        self.emit_line("}");
        self.emit_line("");

        self.emit_line("int32_t String_length(const char* s) { return s ? (int32_t)strlen(s) : 0; }");
        self.emit_line("char* String_toUpperCase(const char* s) {");
        self.emit_line("    if (!s) return NULL;");
        self.emit_line("    char* r = strdup(s);");
        self.emit_line("    for (char* p = r; *p; p++) *p = (*p >= 'a' && *p <= 'z') ? *p - 32 : *p;");
        self.emit_line("    return r;");
        self.emit_line("}");
        self.emit_line("char* String_toLowerCase(const char* s) {");
        self.emit_line("    if (!s) return NULL;");
        self.emit_line("    char* r = strdup(s);");
        self.emit_line("    for (char* p = r; *p; p++) *p = (*p >= 'A' && *p <= 'Z') ? *p + 32 : *p;");
        self.emit_line("    return r;");
        self.emit_line("}");
        self.emit_line("int String_equals(const char* a, const char* b) {");
        self.emit_line("    if (!a || !b) return a == b;");
        self.emit_line("    return strcmp(a, b) == 0;");
        self.emit_line("}");
        self.emit_line("char* String_trim(const char* s) {");
        self.emit_line("    if (!s) return NULL;");
        self.emit_line("    while (*s == ' ' || *s == '\\t' || *s == '\\n' || *s == '\\r') s++;");
        self.emit_line("    if (!*s) return strdup(\"\");");
        self.emit_line("    const char* end = s + strlen(s) - 1;");
        self.emit_line("    while (end > s && (*end == ' ' || *end == '\\t' || *end == '\\n' || *end == '\\r')) end--;");
        self.emit_line("    size_t len = end - s + 1;");
        self.emit_line("    char* r = (char*)malloc(len + 1);");
        self.emit_line("    memcpy(r, s, len);");
        self.emit_line("    r[len] = '\\0';");
        self.emit_line("    return r;");
        self.emit_line("}");
        self.emit_line("char* String_substring(const char* s, int start, int length) {");
        self.emit_line("    if (!s) return NULL;");
        self.emit_line("    int slen = (int)strlen(s);");
        self.emit_line("    if (start < 0) start = 0;");
        self.emit_line("    if (start + length > slen) length = slen - start;");
        self.emit_line("    if (length <= 0) return strdup(\"\");");
        self.emit_line("    char* r = (char*)malloc(length + 1);");
        self.emit_line("    memcpy(r, s + start, length);");
        self.emit_line("    r[length] = '\\0';");
        self.emit_line("    return r;");
        self.emit_line("}");
        self.emit_line("int String_indexOf(const char* s, const char* sub) {");
        self.emit_line("    if (!s || !sub) return -1;");
        self.emit_line("    char* p = strstr(s, sub);");
        self.emit_line("    return p ? (int)(p - s) : -1;");
        self.emit_line("}");
        self.emit_line("char* String_replace(const char* s, const char* old, const char* new_s) {");
        self.emit_line("    if (!s || !old || !new_s) return s ? strdup(s) : NULL;");
        self.emit_line("    size_t slen = strlen(s), olen = strlen(old), nlen = strlen(new_s);");
        self.emit_line("    size_t count = 0;");
        self.emit_line("    const char* p = s;");
        self.emit_line("    while ((p = strstr(p, old))) { count++; p += olen; }");
        self.emit_line("    size_t rlen = slen + count * (nlen - olen);");
        self.emit_line("    char* r = (char*)malloc(rlen + 1);");
        self.emit_line("    char* dst = r;");
        self.emit_line("    p = s;");
        self.emit_line("    while (*p) {");
        self.emit_line("        if (strncmp(p, old, olen) == 0) {");
        self.emit_line("            memcpy(dst, new_s, nlen); dst += nlen; p += olen;");
        self.emit_line("        } else { *dst++ = *p++; }");
        self.emit_line("    }");
        self.emit_line("    *dst = '\\0';");
        self.emit_line("    return r;");
        self.emit_line("}");
        self.emit_line("char* String_intToString(int32_t v) {");
        self.emit_line("    char buf[32];");
        self.emit_line("    snprintf(buf, sizeof(buf), \"%d\", v);");
        self.emit_line("    return strdup(buf);");
        self.emit_line("}");
        self.emit_line("char* String_longToString(int64_t v) {");
        self.emit_line("    char buf[32];");
        self.emit_line("    snprintf(buf, sizeof(buf), \"%I64d\", v);");
        self.emit_line("    return strdup(buf);");
        self.emit_line("}");
        self.emit_line("int32_t String_toInt(const char* s) {");
        self.emit_line("    if (!s) return 0;");
        self.emit_line("    return (int32_t)atoi(s);");
        self.emit_line("}");
        self.emit_line("int64_t String_toLong(const char* s) {");
        self.emit_line("    if (!s) return 0;");
        self.emit_line("    return (int64_t)atoll(s);");
        self.emit_line("}");
        self.emit_line("double String_toDouble(const char* s) {");
        self.emit_line("    if (!s) return 0.0;");
        self.emit_line("    return atof(s);");
        self.emit_line("}");
        self.emit_line("char* String_doubleToString(double v) {");
        self.emit_line("    char* r = (char*)malloc(32);");
        self.emit_line("    snprintf(r, 32, \"%g\", v);");
        self.emit_line("    return r;");
        self.emit_line("}");
        self.emit_line("char* File_readAll(const char* path) {");
        self.emit_line("    FILE* f = fopen(path, \"rb\");");
        self.emit_line("    if (!f) return NULL;");
        self.emit_line("    fseek(f, 0, SEEK_END);");
        self.emit_line("    long sz = ftell(f);");
        self.emit_line("    fseek(f, 0, SEEK_SET);");
        self.emit_line("    char* buf = (char*)malloc(sz + 1);");
        self.emit_line("    size_t rd = fread(buf, 1, sz, f);");
        self.emit_line("    buf[rd] = '\\0';");
        self.emit_line("    fclose(f);");
        self.emit_line("    return buf;");
        self.emit_line("}");
        self.emit_line("char* String_concat(const char* a, const char* b) {");
        self.emit_line("    if (!a && !b) return strdup(\"\");");
        self.emit_line("    if (!a) return strdup(b);");
        self.emit_line("    if (!b) return strdup(a);");
        self.emit_line("    size_t alen = strlen(a), blen = strlen(b);");
        self.emit_line("    char* r = (char*)malloc(alen + blen + 1);");
        self.emit_line("    memcpy(r, a, alen);");
        self.emit_line("    memcpy(r + alen, b, blen + 1);");
        self.emit_line("    return r;");
        self.emit_line("}");
        self.emit_line("char String_charAt(const char* s, int idx) {");
        self.emit_line("    if (!s || idx < 0 || idx >= (int)strlen(s)) return '\\0';");
        self.emit_line("    return s[idx];");
        self.emit_line("}");
        self.emit_line("");

        self.emit_line("typedef struct StringBuilder {");
        self.emit_line("    char* buffer;");
        self.emit_line("    int32_t length;");
        self.emit_line("    int32_t capacity;");
        self.emit_line("} StringBuilder;");
        self.emit_line("");
        self.emit_line("StringBuilder* StringBuilder_new() {");
        self.emit_line("    StringBuilder* sb = (StringBuilder*)malloc(sizeof(StringBuilder));");
        self.emit_line("    sb->capacity = 64;");
        self.emit_line("    sb->length = 0;");
        self.emit_line("    sb->buffer = (char*)malloc(sb->capacity);");
        self.emit_line("    sb->buffer[0] = '\\0';");
        self.emit_line("    return sb;");
        self.emit_line("}");
        self.emit_line("void StringBuilder_ensureCapacity(StringBuilder* sb, int32_t needed) {");
        self.emit_line("    if (sb->length + needed >= sb->capacity) {");
        self.emit_line("        while (sb->length + needed >= sb->capacity) sb->capacity *= 2;");
        self.emit_line("        sb->buffer = (char*)realloc(sb->buffer, sb->capacity);");
        self.emit_line("    }");
        self.emit_line("}");
        self.emit_line("void StringBuilder_append(StringBuilder* sb, const char* s) {");
        self.emit_line("    if (!s) return;");
        self.emit_line("    int32_t slen = (int32_t)strlen(s);");
        self.emit_line("    StringBuilder_ensureCapacity(sb, slen);");
        self.emit_line("    memcpy(sb->buffer + sb->length, s, slen + 1);");
        self.emit_line("    sb->length += slen;");
        self.emit_line("}");
        self.emit_line("void StringBuilder_appendChar(StringBuilder* sb, char c) {");
        self.emit_line("    StringBuilder_ensureCapacity(sb, 1);");
        self.emit_line("    sb->buffer[sb->length++] = c;");
        self.emit_line("    sb->buffer[sb->length] = '\\0';");
        self.emit_line("}");
        self.emit_line("void StringBuilder_appendInt(StringBuilder* sb, int32_t v) {");
        self.emit_line("    char tmp[32];");
        self.emit_line("    snprintf(tmp, sizeof(tmp), \"%d\", v);");
        self.emit_line("    StringBuilder_append(sb, tmp);");
        self.emit_line("}");
        self.emit_line("void StringBuilder_appendLong(StringBuilder* sb, int64_t v) {");
        self.emit_line("    char tmp[32];");
        self.emit_line("    snprintf(tmp, sizeof(tmp), \"%\" PRId64, v);");
        self.emit_line("    StringBuilder_append(sb, tmp);");
        self.emit_line("}");
        self.emit_line("void StringBuilder_appendFloat(StringBuilder* sb, float v) {");
        self.emit_line("    char tmp[32];");
        self.emit_line("    snprintf(tmp, sizeof(tmp), \"%g\", (double)v);");
        self.emit_line("    StringBuilder_append(sb, tmp);");
        self.emit_line("}");
        self.emit_line("void StringBuilder_appendDouble(StringBuilder* sb, double v) {");
        self.emit_line("    char tmp[32];");
        self.emit_line("    snprintf(tmp, sizeof(tmp), \"%g\", v);");
        self.emit_line("    StringBuilder_append(sb, tmp);");
        self.emit_line("}");
        self.emit_line("void StringBuilder_appendBool(StringBuilder* sb, int32_t v) {");
        self.emit_line("    StringBuilder_append(sb, v ? \"true\" : \"false\");");
        self.emit_line("}");
        self.emit_line("const char* StringBuilder_toString(StringBuilder* sb) {");
        self.emit_line("    return strdup(sb->buffer);");
        self.emit_line("}");
        self.emit_line("int32_t StringBuilder_length(StringBuilder* sb) {");
        self.emit_line("    return sb->length;");
        self.emit_line("}");
        self.emit_line("void StringBuilder_setCharAt(StringBuilder* sb, int32_t idx, char c) {");
        self.emit_line("    if (idx >= 0 && idx < sb->length) sb->buffer[idx] = c;");
        self.emit_line("}");
        self.emit_line("char StringBuilder_charAt(StringBuilder* sb, int32_t idx) {");
        self.emit_line("    if (idx >= 0 && idx < sb->length) return sb->buffer[idx];");
        self.emit_line("    return '\\0';");
        self.emit_line("}");
        self.emit_line("void StringBuilder_deleteCharAt(StringBuilder* sb, int32_t idx) {");
        self.emit_line("    if (idx >= 0 && idx < sb->length) {");
        self.emit_line("        memmove(sb->buffer + idx, sb->buffer + idx + 1, sb->length - idx);");
        self.emit_line("        sb->length--;");
        self.emit_line("    }");
        self.emit_line("}");
        self.emit_line("void StringBuilder_insert(StringBuilder* sb, int32_t idx, const char* s) {");
        self.emit_line("    if (!s || idx < 0 || idx > sb->length) return;");
        self.emit_line("    int32_t slen = (int32_t)strlen(s);");
        self.emit_line("    StringBuilder_ensureCapacity(sb, slen);");
        self.emit_line("    memmove(sb->buffer + idx + slen, sb->buffer + idx, sb->length - idx + 1);");
        self.emit_line("    memcpy(sb->buffer + idx, s, slen);");
        self.emit_line("    sb->length += slen;");
        self.emit_line("}");
        self.emit_line("void StringBuilder_clear(StringBuilder* sb) {");
        self.emit_line("    sb->length = 0;");
        self.emit_line("    sb->buffer[0] = '\\0';");
        self.emit_line("}");
        self.emit_line("void StringBuilder_free(StringBuilder* sb) {");
        self.emit_line("    free(sb->buffer);");
        self.emit_line("    free(sb);");
        self.emit_line("}");
        self.emit_line("");

        self.emit_line("typedef struct LemonFile {");
        self.emit_line("    FILE* handle;");
        self.emit_line("} LemonFile;");
        self.emit_line("");
        self.emit_line("LemonFile* File_open(const char* path, const char* mode) {");
        self.emit_line("    FILE* f = fopen(path, mode);");
        self.emit_line("    if (!f) return NULL;");
        self.emit_line("    LemonFile* lf = (LemonFile*)malloc(sizeof(LemonFile));");
        self.emit_line("    lf->handle = f;");
        self.emit_line("    return lf;");
        self.emit_line("}");
        self.emit_line("void File_close(LemonFile* lf) {");
        self.emit_line("    if (lf && lf->handle) { fclose(lf->handle); lf->handle = NULL; }");
        self.emit_line("}");
        self.emit_line("const char* LemonFile_readAll(LemonFile* lf) {");
        self.emit_line("    if (!lf || !lf->handle) return strdup(\"\");");
        self.emit_line("    fseek(lf->handle, 0, SEEK_END);");
        self.emit_line("    long sz = ftell(lf->handle);");
        self.emit_line("    fseek(lf->handle, 0, SEEK_SET);");
        self.emit_line("    char* buf = (char*)malloc(sz + 1);");
        self.emit_line("    size_t rd = fread(buf, 1, sz, lf->handle);");
        self.emit_line("    buf[rd] = '\\0';");
        self.emit_line("    return buf;");
        self.emit_line("}");
        self.emit_line("const char* File_readLine(LemonFile* lf) {");
        self.emit_line("    if (!lf || !lf->handle) return strdup(\"\");");
        self.emit_line("    char buf[4096];");
        self.emit_line("    if (!fgets(buf, sizeof(buf), lf->handle)) return NULL;");
        self.emit_line("    int32_t len = (int32_t)strlen(buf);");
        self.emit_line("    if (len > 0 && buf[len-1] == '\\n') buf[--len] = '\\0';");
        self.emit_line("    if (len > 0 && buf[len-1] == '\\r') buf[--len] = '\\0';");
        self.emit_line("    return strdup(buf);");
        self.emit_line("}");
        self.emit_line("void File_write(LemonFile* lf, const char* s) {");
        self.emit_line("    if (lf && lf->handle && s) fputs(s, lf->handle);");
        self.emit_line("}");
        self.emit_line("void File_writeLine(LemonFile* lf, const char* s) {");
        self.emit_line("    if (lf && lf->handle && s) { fputs(s, lf->handle); fputc('\\n', lf->handle); }");
        self.emit_line("}");
        self.emit_line("int32_t File_hasNextLine(LemonFile* lf) {");
        self.emit_line("    if (!lf || !lf->handle) return 0;");
        self.emit_line("    int c = fgetc(lf->handle);");
        self.emit_line("    if (c == EOF) return 0;");
        self.emit_line("    ungetc(c, lf->handle);");
        self.emit_line("    return 1;");
        self.emit_line("}");
        self.emit_line("int32_t File_eof(LemonFile* lf) {");
        self.emit_line("    if (!lf || !lf->handle) return 1;");
        self.emit_line("    return feof(lf->handle);");
        self.emit_line("}");
        self.emit_line("");

        self.emit_line("int32_t Character_isDigit(char c) { return c >= '0' && c <= '9'; }");
        self.emit_line("int32_t Character_isLetter(char c) { return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z'); }");
        self.emit_line("int32_t Character_isLetterOrDigit(char c) { return Character_isLetter(c) || Character_isDigit(c); }");
        self.emit_line("int32_t Character_isWhitespace(char c) { return c == ' ' || c == '\\t' || c == '\\n' || c == '\\r'; }");
        self.emit_line("int32_t Character_isUpperCase(char c) { return c >= 'A' && c <= 'Z'; }");
        self.emit_line("int32_t Character_isLowerCase(char c) { return c >= 'a' && c <= 'z'; }");
        self.emit_line("char Character_toUpperCase(char c) { if (c >= 'a' && c <= 'z') return c - 32; return c; }");
        self.emit_line("char Character_toLowerCase(char c) { if (c >= 'A' && c <= 'Z') return c + 32; return c; }");
        self.emit_line("int32_t Character_getNumericValue(char c) { if (c >= '0' && c <= '9') return c - '0'; return -1; }");
        self.emit_line("int32_t Character_isAlpha(char c) { return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || c == '_'; }");
        self.emit_line("int32_t Character_isAlphaNumeric(char c) { return Character_isAlpha(c) || Character_isDigit(c); }");
        self.emit_line("int32_t Character_isPrintable(char c) { return c >= 32 && c <= 126; }");
        self.emit_line("char Character_fromInt(int32_t v) { return (char)v; }");
        self.emit_line("int32_t Character_toInt(char c) { return (int32_t)c; }");
        self.emit_line("");

        self.emit_line("const char* String_fromCharArray(const char* buf, int32_t start, int32_t len) {");
        self.emit_line("    if (!buf || len <= 0) return strdup(\"\");");
        self.emit_line("    char* r = (char*)malloc(len + 1);");
        self.emit_line("    memcpy(r, buf + start, len);");
        self.emit_line("    r[len] = '\\0';");
        self.emit_line("    return r;");
        self.emit_line("}");
        self.emit_line("const char* String_fromChar(char c) {");
        self.emit_line("    char* r = (char*)malloc(2);");
        self.emit_line("    r[0] = c;");
        self.emit_line("    r[1] = '\\0';");
        self.emit_line("    return r;");
        self.emit_line("}");
        self.emit_line("int32_t String_codePointAt(const char* s, int32_t idx) {");
        self.emit_line("    if (!s || idx < 0 || idx >= (int32_t)strlen(s)) return -1;");
        self.emit_line("    return (int32_t)(unsigned char)s[idx];");
        self.emit_line("}");
        self.emit_line("int32_t String_startsWith(const char* s, const char* prefix) {");
        self.emit_line("    if (!s || !prefix) return 0;");
        self.emit_line("    return strncmp(s, prefix, strlen(prefix)) == 0;");
        self.emit_line("}");
        self.emit_line("int32_t String_endsWith(const char* s, const char* suffix) {");
        self.emit_line("    if (!s || !suffix) return 0;");
        self.emit_line("    int32_t slen = (int32_t)strlen(s);");
        self.emit_line("    int32_t suflen = (int32_t)strlen(suffix);");
        self.emit_line("    if (suflen > slen) return 0;");
        self.emit_line("    return strcmp(s + slen - suflen, suffix) == 0;");
        self.emit_line("}");
        self.emit_line("typedef struct Array_String {");
        self.emit_line("    int32_t length;");
        self.emit_line("    int32_t capacity;");
        self.emit_line("    void* data;");
        self.emit_line("} Array_String;");
        self.emit_line("Array_String* String_split(const char* s, char delim) {");
        self.emit_line("    Array_String* result = (Array_String*)malloc(sizeof(Array_String));");
        self.emit_line("    result->length = 0;");
        self.emit_line("    result->capacity = 8;");
        self.emit_line("    result->data = malloc(sizeof(const char*) * result->capacity);");
        self.emit_line("    if (!s) return result;");
        self.emit_line("    const char* start = s;");
        self.emit_line("    while (*start) {");
        self.emit_line("        const char* end = start;");
        self.emit_line("        while (*end && *end != delim) end++;");
        self.emit_line("        int32_t len = (int32_t)(end - start);");
        self.emit_line("        char* part = (char*)malloc(len + 1);");
        self.emit_line("        memcpy(part, start, len);");
        self.emit_line("        part[len] = '\\0';");
        self.emit_line("        if (result->length >= result->capacity) {");
        self.emit_line("            result->capacity *= 2;");
        self.emit_line("            result->data = realloc(result->data, sizeof(const char*) * result->capacity);");
        self.emit_line("        }");
        self.emit_line("        ((const char**)result->data)[result->length++] = part;");
        self.emit_line("        if (*end) end++;");
        self.emit_line("        start = end;");
        self.emit_line("    }");
        self.emit_line("    return result;");
        self.emit_line("}");
        self.emit_line("");

        self.emit_line("typedef struct LemonArray {");
        self.emit_line("    int32_t length;");
        self.emit_line("    int32_t capacity;");
        self.emit_line("    int32_t elem_size;");
        self.emit_line("    void* data;");
        self.emit_line("} LemonArray;");
        self.emit_line("");
        self.emit_line("LemonArray* LemonArray_new(int32_t elem_size) {");
        self.emit_line("    LemonArray* arr = (LemonArray*)malloc(sizeof(LemonArray));");
        self.emit_line("    arr->length = 0;");
        self.emit_line("    arr->capacity = 8;");
        self.emit_line("    arr->elem_size = elem_size;");
        self.emit_line("    arr->data = malloc(arr->capacity * elem_size);");
        self.emit_line("    return arr;");
        self.emit_line("}");
        self.emit_line("");
        self.emit_line("void LemonArray_ensureCapacity(LemonArray* arr, int32_t min_cap) {");
        self.emit_line("    if (arr->capacity >= min_cap) return;");
        self.emit_line("    while (arr->capacity < min_cap) arr->capacity *= 2;");
        self.emit_line("    arr->data = realloc(arr->data, arr->capacity * arr->elem_size);");
        self.emit_line("}");
        self.emit_line("");
        self.emit_line("void LemonArray_add(LemonArray* arr, void* elem) {");
        self.emit_line("    LemonArray_ensureCapacity(arr, arr->length + 1);");
        self.emit_line("    memcpy((char*)arr->data + arr->length * arr->elem_size, &elem, arr->elem_size);");
        self.emit_line("    arr->length++;");
        self.emit_line("}");
        self.emit_line("void LemonArray_push(LemonArray* arr, void* elem) { LemonArray_add(arr, elem); }");
        self.emit_line("");
        self.emit_line("void* LemonArray_get(LemonArray* arr, int32_t index) {");
        self.emit_line("    if (index < 0 || index >= arr->length) return NULL;");
        self.emit_line("    void* result;");
        self.emit_line("    memcpy(&result, (char*)arr->data + index * arr->elem_size, arr->elem_size);");
        self.emit_line("    return result;");
        self.emit_line("}");
        self.emit_line("");
        self.emit_line("void LemonArray_set(LemonArray* arr, int32_t index, void* elem) {");
        self.emit_line("    if (index < 0 || index >= arr->length) return;");
        self.emit_line("    memcpy((char*)arr->data + index * arr->elem_size, &elem, arr->elem_size);");
        self.emit_line("}");
        self.emit_line("");
        self.emit_line("int32_t LemonArray_size(LemonArray* arr) { return arr ? arr->length : 0; }");
        self.emit_line("");
        self.emit_line("void LemonArray_removeAt(LemonArray* arr, int32_t index) {");
        self.emit_line("    if (index < 0 || index >= arr->length) return;");
        self.emit_line("    memmove((char*)arr->data + index * arr->elem_size,");
        self.emit_line("            (char*)arr->data + (index + 1) * arr->elem_size,");
        self.emit_line("            (arr->length - index - 1) * arr->elem_size);");
        self.emit_line("    arr->length--;");
        self.emit_line("}");
        self.emit_line("");
        self.emit_line("char* String_join(LemonArray* arr, const char* sep) {");
        self.emit_line("    if (!arr || arr->length == 0) return strdup(\"\");");
        self.emit_line("    if (!sep) sep = \"\";");
        self.emit_line("    int32_t sep_len = (int32_t)strlen(sep);");
        self.emit_line("    int32_t total = 0;");
        self.emit_line("    for (int32_t i = 0; i < arr->length; i++) {");
        self.emit_line("        void* elem = LemonArray_get(arr, i);");
        self.emit_line("        const char* s = elem ? (const char*)elem : \"\";");
        self.emit_line("        total += (int32_t)strlen(s);");
        self.emit_line("        if (i > 0) total += sep_len;");
        self.emit_line("    }");
        self.emit_line("    char* r = (char*)malloc(total + 1);");
        self.emit_line("    r[0] = '\\0';");
        self.emit_line("    char* p = r;");
        self.emit_line("    for (int32_t i = 0; i < arr->length; i++) {");
        self.emit_line("        void* elem = LemonArray_get(arr, i);");
        self.emit_line("        const char* s = elem ? (const char*)elem : \"\";");
        self.emit_line("        if (i > 0 && sep_len > 0) { memcpy(p, sep, sep_len); p += sep_len; }");
        self.emit_line("        int32_t slen = (int32_t)strlen(s);");
        self.emit_line("        memcpy(p, s, slen); p += slen;");
        self.emit_line("    }");
        self.emit_line("    *p = '\\0';");
        self.emit_line("    return r;");
        self.emit_line("}");
        self.emit_line("");

        self.emit_line("typedef struct LemonMapEntry {");
        self.emit_line("    char* key;");
        self.emit_line("    void* value;");
        self.emit_line("    int occupied;");
        self.emit_line("} LemonMapEntry;");
        self.emit_line("");
        self.emit_line("typedef struct LemonMap {");
        self.emit_line("    int32_t size;");
        self.emit_line("    int32_t capacity;");
        self.emit_line("    LemonMapEntry* entries;");
        self.emit_line("} LemonMap;");
        self.emit_line("");
        self.emit_line("static uint32_t _map_hash(const char* key) {");
        self.emit_line("    uint32_t h = 2166136261u;");
        self.emit_line("    for (const char* p = key; *p; p++) { h ^= (uint8_t)*p; h *= 16777619u; }");
        self.emit_line("    return h;");
        self.emit_line("}");
        self.emit_line("");
        self.emit_line("LemonMap* LemonMap_new() {");
        self.emit_line("    LemonMap* map = (LemonMap*)malloc(sizeof(LemonMap));");
        self.emit_line("    map->size = 0;");
        self.emit_line("    map->capacity = 16;");
        self.emit_line("    map->entries = (LemonMapEntry*)calloc(map->capacity, sizeof(LemonMapEntry));");
        self.emit_line("    return map;");
        self.emit_line("}");
        self.emit_line("");
        self.emit_line("void LemonMap_put(LemonMap* map, const char* key, void* value) {");
        self.emit_line("    if (map->size * 2 >= map->capacity) {");
        self.emit_line("        int32_t old_cap = map->capacity;");
        self.emit_line("        LemonMapEntry* old = map->entries;");
        self.emit_line("        map->capacity *= 2;");
        self.emit_line("        map->entries = (LemonMapEntry*)calloc(map->capacity, sizeof(LemonMapEntry));");
        self.emit_line("        map->size = 0;");
        self.emit_line("        for (int32_t i = 0; i < old_cap; i++) {");
        self.emit_line("            if (old[i].occupied) LemonMap_put(map, old[i].key, old[i].value);");
        self.emit_line("        }");
        self.emit_line("        free(old);");
        self.emit_line("        LemonMap_put(map, key, value);");
        self.emit_line("        return;");
        self.emit_line("    }");
        self.emit_line("    uint32_t idx = _map_hash(key) % map->capacity;");
        self.emit_line("    while (map->entries[idx].occupied) {");
        self.emit_line("        if (strcmp(map->entries[idx].key, key) == 0) {");
        self.emit_line("            map->entries[idx].value = value;");
        self.emit_line("            return;");
        self.emit_line("        }");
        self.emit_line("        idx = (idx + 1) % map->capacity;");
        self.emit_line("    }");
        self.emit_line("    map->entries[idx].key = strdup(key);");
        self.emit_line("    map->entries[idx].value = value;");
        self.emit_line("    map->entries[idx].occupied = 1;");
        self.emit_line("    map->size++;");
        self.emit_line("}");
        self.emit_line("");
        self.emit_line("void* LemonMap_get(LemonMap* map, const char* key) {");
        self.emit_line("    if (!map || map->size == 0) return NULL;");
        self.emit_line("    uint32_t idx = _map_hash(key) % map->capacity;");
        self.emit_line("    for (int32_t i = 0; i < map->capacity; i++) {");
        self.emit_line("        if (!map->entries[idx].occupied) return NULL;");
        self.emit_line("        if (strcmp(map->entries[idx].key, key) == 0) return map->entries[idx].value;");
        self.emit_line("        idx = (idx + 1) % map->capacity;");
        self.emit_line("    }");
        self.emit_line("    return NULL;");
        self.emit_line("}");
        self.emit_line("");
        self.emit_line("int32_t LemonMap_size(LemonMap* map) { return map ? map->size : 0; }");
        self.emit_line("");
        self.emit_line("int LemonMap_containsKey(LemonMap* map, const char* key) {");
        self.emit_line("    return LemonMap_get(map, key) != NULL;");
        self.emit_line("}");
        self.emit_line("");
        self.emit_line("void LemonMap_remove(LemonMap* map, const char* key) {");
        self.emit_line("    if (!map || map->size == 0) return;");
        self.emit_line("    uint32_t idx = _map_hash(key) % map->capacity;");
        self.emit_line("    for (int32_t i = 0; i < map->capacity; i++) {");
        self.emit_line("        if (!map->entries[idx].occupied) return;");
        self.emit_line("        if (strcmp(map->entries[idx].key, key) == 0) {");
        self.emit_line("            free(map->entries[idx].key);");
        self.emit_line("            map->entries[idx].key = NULL;");
        self.emit_line("            map->entries[idx].value = NULL;");
        self.emit_line("            map->entries[idx].occupied = 0;");
        self.emit_line("            map->size--;");
        self.emit_line("            return;");
        self.emit_line("        }");
        self.emit_line("        idx = (idx + 1) % map->capacity;");
        self.emit_line("    }");
        self.emit_line("}");
        self.emit_line("");

        for decl in &ast.declarations {
            if let Declaration::Class(class) = decl {
                self.emit_line(&format!("typedef struct {} {};", class.name, class.name));
            }
            if let Declaration::Enum(enum_decl) = decl {
                self.enums.insert(enum_decl.name.clone(), enum_decl.clone());
                self.emit_line(&format!("typedef struct {} {};", enum_decl.name, enum_decl.name));
            }
        }
        let generic_fwd_decl_lines: Vec<String> = {
            let mut lines = Vec::new();
            for (class_name, instances) in &self.generic_instances {
                if self.generic_classes.contains_key(class_name) {
                    for type_args in instances {
                        let mangled = self.mangled_generic_name(class_name, type_args);
                        lines.push(format!("typedef struct {} {};", mangled, mangled));
                    }
                }
            }
            lines
        };
        for line in &generic_fwd_decl_lines {
            self.emit_line(line);
        }
        self.emit_line("");

        let vtable_info: Vec<(String, Vec<(String, String, String)>)> = self.virtual_methods.iter()
            .filter(|(_, v)| !v.is_empty())
            .map(|(cn, vms)| {
                let entries: Vec<(String, String, String)> = vms.iter().map(|vm| {
                    let field_name = mangle_method_name(cn, &vm.name, &vm.params);
                    (self.c_type(&vm.return_type), field_name, self.c_params_with_this(&vm.params, cn))
                }).collect();
                (cn.clone(), entries)
            })
            .collect();

        for (class_name, entries) in &vtable_info {
            self.emit_line(&format!("typedef struct {}_vtable {{", class_name));
            self.indent += 1;
            for (ret, name, params) in entries {
                self.emit_line(&format!("{} (*{})({});", ret, name, params));
            }
            self.indent -= 1;
            self.emit_line(&format!("}} {}_vtable;", class_name));
            self.emit_line("");
        }

        let itable_info: Vec<(String, Vec<(String, String, String)>)> = self.interface_methods.iter()
            .map(|(iface_name, methods)| {
                let entries: Vec<(String, String, String)> = methods.iter().map(|m| {
                    (self.c_type(&m.return_type), m.name.clone(), self.c_params_with_void_this(&m.params))
                }).collect();
                (iface_name.clone(), entries)
            })
            .collect();

        for (iface_name, entries) in &itable_info {
            self.emit_line(&format!("typedef struct {}_itable {{", iface_name));
            self.indent += 1;
            for (ret, name, params) in entries {
                self.emit_line(&format!("{} (*{})({});", ret, name, params));
            }
            self.indent -= 1;
            self.emit_line(&format!("}} {}_itable;", iface_name));
            self.emit_line("");
        }

        for decl in &ast.declarations {
            if let Declaration::Class(class) = decl {
                self.generate_struct_def(class);
            }
            if let Declaration::Enum(enum_decl) = decl {
                self.generate_enum_struct_def(enum_decl);
            }
        }
        let generic_struct_items: Vec<(ClassDecl, Vec<TypeRef>)> = {
            let mut items = Vec::new();
            for (class_name, instances) in &self.generic_instances {
                if let Some(gc) = self.generic_classes.get(class_name) {
                    for ta in instances {
                        items.push((gc.clone(), ta.clone()));
                    }
                }
            }
            items
        };
        for (generic_class, type_args) in &generic_struct_items {
            self.generate_generic_struct_def(generic_class, type_args);
        }
        self.emit_line("");

        for decl in &ast.declarations {
            if let Declaration::Class(class) = decl {
                let has_ctor = self.class_has_explicit_ctor(class);
                if !has_ctor {
                    self.emit_line(&format!(
                        "{}* {}_ctor({}* self);",
                        class.name, class.name, class.name
                    ));
                }
                for member in &class.members {
                    match member {
                        ClassMember::Constructor(ctor) => {
                            self.emit_line(&format!(
                                "{}* {}_ctor({});",
                                class.name,
                                class.name,
                                self.c_params_with_this_ctor(&ctor.params, &class.name)
                            ));
                        }
                        ClassMember::Method(method) => {
                            if method.name == class.name {
                                self.emit_line(&format!(
                                    "{}* {}_ctor({});",
                                    class.name,
                                    class.name,
                                    self.c_params_with_this_ctor(&method.params, &class.name)
                                ));
                            } else {
                                let mangled = mangle_method_name(&class.name, &method.name, &method.params);
                                self.emit_line(&format!(
                                    "{} {}({});",
                                    self.c_type(&method.return_type),
                                    mangled,
                                    self.c_params_with_this(&method.params, &class.name)
                                ));
                            }
                        }
                        ClassMember::Destructor(_) => {
                            self.emit_line(&format!(
                                "void {}_dtor({}* self);",
                                class.name, class.name
                            ));
                        }
                        _ => {}
                    }
                }
            }
        }
        for decl in &ast.declarations {
            if let Declaration::Function(func) = decl {
                self.emit_line(&format!(
                    "{} {}({});",
                    self.c_type(&func.return_type),
                    func.name,
                    self.c_params(&func.params)
                ));
            }
        }
        for (generic_class, type_args) in &generic_struct_items {
            self.emit_generic_forward_decls(generic_class, type_args);
        }
        self.emit_line("");

        let vtable_instances: Vec<(String, Vec<String>)> = ast.declarations.iter()
            .filter_map(|decl| {
                if let Declaration::Class(class) = decl {
                    if let Some(vmethods) = self.virtual_methods.get(&class.name) {
                        if !vmethods.is_empty() {
                            let entries: Vec<String> = vmethods.iter().map(|vm| {
                                mangle_method_name(&class.name, &vm.name, &vm.params)
                            }).collect();
                            return Some((class.name.clone(), entries));
                        }
                    }
                }
                None
            })
            .collect();

        for (class_name, entries) in &vtable_instances {
            self.emit_line(&format!(
                "static {}_vtable _{}_vtable = {{",
                class_name, class_name
            ));
            self.indent += 1;
            self.emit_line(&entries.join(",\n"));
            self.indent -= 1;
            self.emit_line("};");
            self.emit_line("");
        }

        let itable_methods = self.interface_methods.clone();
        let itable_instances: Vec<(String, String, Vec<String>)> = self.class_implements.iter()
            .flat_map(|(class_name, iface_names)| {
                let cn = class_name.clone();
                let im = itable_methods.clone();
                let sigs = self.method_signatures.clone();
                iface_names.iter().filter_map(move |iface_name| {
                    if let Some(methods) = im.get(iface_name) {
                        let entries: Vec<String> = methods.iter().map(|m| {
                            let mangled = if let Some(class_sigs) = sigs.get(&cn) {
                                class_sigs.iter()
                                    .filter(|(name, _)| name == &m.name)
                                    .map(|(name, param_types)| {
                                        let params: Vec<Param> = param_types.iter().map(|pt| Param {
                                            param_type: pt.clone(),
                                            name: String::new(),
                                            default_value: None,
                                        }).collect();
                                        mangle_method_name(&cn, name, &params)
                                    })
                                    .next()
                                    .unwrap_or_else(|| format!("{}_{}", cn, m.name))
                            } else {
                                format!("{}_{}", cn, m.name)
                            };
                            format!("(void(*)(void*)){}", mangled)
                        }).collect();
                        Some((cn.clone(), iface_name.clone(), entries))
                    } else {
                        None
                    }
                })
            })
            .collect();

        for (class_name, iface_name, entries) in &itable_instances {
            self.emit_line(&format!(
                "static {}_itable _{}_{}_itable = {{",
                iface_name, class_name, iface_name
            ));
            self.indent += 1;
            self.emit_line(&entries.join(",\n"));
            self.indent -= 1;
            self.emit_line("};");
            self.emit_line("");
        }

        for decl in &ast.declarations {
            match decl {
                Declaration::Class(class) => {
                    for member in &class.members {
                        if let ClassMember::Field(field) = member {
                            let is_static = field.modifiers.iter().any(|m| matches!(m, VarModifier::Static));
                            if is_static {
                                if let Some(init) = &field.initializer {
                                    let val = self.gen_expr(init);
                                    self.emit_line(&format!("#define {}_{} {}", class.name.to_uppercase(), field.name, val));
                                }
                            }
                        }
                    }
                    self.generate_class(class);
                }
                Declaration::Function(func) => self.generate_function(func),
                _ => {}
            }
        }

        for (generic_class, type_args) in &generic_struct_items {
            self.generate_generic_class(generic_class, type_args);
        }

        for decl in &ast.declarations {
            if let Declaration::Class(class) = decl {
                for member in &class.members {
                    if let ClassMember::Method(method) = member {
                        if method.name == "main" && method.modifiers.iter().any(|m| matches!(m, MethodModifier::Static)) {
                            let mangled = mangle_method_name(&class.name, &method.name, &method.params);
                            self.emit_line("int main(int argc, char** argv) {");
                            self.indent += 1;
                            self.emit_line(&format!(
                                "{}(NULL, argv);",
                                mangled
                            ));
                            self.emit_line("return 0;");
                            self.indent -= 1;
                            self.emit_line("}");
                            self.emit_line("");
                            return String::from_utf8_lossy(&self.output).to_string();
                        }
                    }
                }
            }
        }

        String::from_utf8_lossy(&self.output).to_string()
    }

    fn collect_virtual_methods(&mut self, ast: &Program) {
        let mut base_virtuals: HashMap<String, Vec<VirtualMethodEntry>> = HashMap::new();

        for decl in &ast.declarations {
            if let Declaration::Class(class) = decl {
                for member in &class.members {
                    if let ClassMember::Method(m) = member {
                        let is_virtual = m.modifiers.iter().any(|m2| matches!(m2, MethodModifier::Virtual));
                        if is_virtual {
                            base_virtuals.entry(class.name.clone()).or_default().push(VirtualMethodEntry {
                                name: m.name.clone(),
                                return_type: m.return_type.clone(),
                                params: m.params.clone(),
                            });
                        }
                    }
                }
            }
        }

        for decl in &ast.declarations {
            if let Declaration::Class(class) = decl {
                let mut vmethods = Vec::new();

                if let Some(parent) = &class.extends {
                    if let TypeRef::Named(parent_name, _) = parent {
                        if let Some(parent_vmethods) = base_virtuals.get(parent_name) {
                            for vm in parent_vmethods {
                                vmethods.push(VirtualMethodEntry {
                                    name: vm.name.clone(),
                                    return_type: vm.return_type.clone(),
                                    params: vm.params.clone(),
                                });
                            }
                        }
                    }
                }

                for vm in base_virtuals.get(&class.name).cloned().unwrap_or_default() {
                    if !vmethods.iter().any(|v| v.name == vm.name && v.params.len() == vm.params.len()) {
                        vmethods.push(vm);
                    }
                }

                self.virtual_methods.insert(class.name.clone(), vmethods);
            }
        }

        for (class_name, vmethods) in &self.virtual_methods {
            if !vmethods.is_empty() {
                for parent_name in self.get_all_parent_classes(class_name, ast) {
                    if let Some(parent_vmethods) = self.virtual_methods.get(&parent_name) {
                        if parent_vmethods.is_empty() && !vmethods.is_empty() {
                            let _ = parent_vmethods;
                        }
                    }
                }
            }
        }
    }

    fn get_all_parent_classes(&self, class_name: &str, ast: &Program) -> Vec<String> {
        let mut result = Vec::new();
        let mut current = class_name.to_string();
        loop {
            let mut found = false;
            for decl in &ast.declarations {
                if let Declaration::Class(class) = decl {
                    if class.name == current {
                        if let Some(parent) = &class.extends {
                            if let TypeRef::Named(parent_name, _) = parent {
                                result.push(parent_name.clone());
                                current = parent_name.clone();
                                found = true;
                                break;
                            }
                        }
                    }
                }
            }
            if !found {
                break;
            }
        }
        result
    }

    fn collect_class_fields(&self, class: &ClassDecl, fields: &mut Vec<(String, String)>) {
        for member in &class.members {
            if let ClassMember::Field(field) = member {
                fields.push((self.c_type(&field.var_type), field.name.clone()));
            }
        }
    }

    fn class_has_explicit_ctor(&self, class: &ClassDecl) -> bool {
        for member in &class.members {
            match member {
                ClassMember::Constructor(_) => return true,
                ClassMember::Method(method) if method.name == class.name => return true,
                _ => {}
            }
        }
        false
    }

    fn is_virtual_method(&self, class_name: &str, method_name: &str, param_count: usize) -> bool {
        if let Some(vmethods) = self.virtual_methods.get(class_name) {
            vmethods.iter().any(|vm| vm.name == method_name && vm.params.len() == param_count)
        } else {
            false
        }
    }

    fn generate_struct_def(&mut self, class: &ClassDecl) {
        self.emit_line(&format!("struct {} {{", class.name));
        self.indent += 1;

        let has_vtable = self.virtual_methods.get(&class.name).map_or(false, |v| !v.is_empty());
        if has_vtable {
            self.emit_line(&format!("{}_vtable* vtable;", class.name));
        } else {
            self.emit_line("void** vtable;");
        }

        let iface_names: Vec<String> = self.class_implements.get(&class.name)
            .cloned()
            .unwrap_or_default();
        for iface_name in &iface_names {
            self.emit_line(&format!("{}_itable* itable_{};", iface_name, iface_name));
        }

        if let Some(parent_name) = class.extends.as_ref().and_then(|t| {
            if let TypeRef::Named(name, _) = t { Some(name.clone()) } else { None }
        }) {
            let parent_fields: Vec<(String, String)> = self.class_fields
                .get(&parent_name)
                .cloned()
                .unwrap_or_default();
            for (field_type, field_name) in &parent_fields {
                self.emit_line(&format!("{} {};", field_type, field_name));
            }
        }

        for member in &class.members {
            if let ClassMember::Field(field) = member {
                self.emit_line(&format!("{} {};", self.c_type(&field.var_type), field.name));
            }
        }

        self.indent -= 1;
        self.emit_line("};");
        self.emit_line("");
    }

    fn generate_class(&mut self, class: &ClassDecl) {
        self.current_class = Some(class.name.clone());
        self.parent_class = class.extends.as_ref().and_then(|t| {
            if let TypeRef::Named(name, _) = t {
                Some(name.clone())
            } else {
                None
            }
        });
        self.var_types.clear();
        self.var_interface_types.clear();

        for member in &class.members {
            if let ClassMember::Field(field) = member {
                self.var_types.insert(field.name.clone(), self.c_type(&field.var_type));
            }
        }

        let has_ctor = self.class_has_explicit_ctor(class);
        if !has_ctor {
            self.generate_default_ctor(class);
        }

        for member in &class.members {
            match member {
                ClassMember::Constructor(ctor) => {
                    self.generate_constructor(ctor, class);
                }
                ClassMember::Method(method) => {
                    if method.name == class.name {
                        self.generate_ctor_method(method, class);
                    } else {
                        self.generate_method(method, class);
                    }
                }
                ClassMember::Destructor(dtor) => {
                    self.generate_destructor(dtor, class);
                }
                _ => {}
            }
        }

        self.current_class = None;
        self.parent_class = None;
    }

    fn emit_vtable_init(&mut self, class: &ClassDecl) {
        let has_vtable = self.virtual_methods.get(&class.name).map_or(false, |v| !v.is_empty());
        if has_vtable {
            self.emit_line(&format!("self->vtable = &_{}_vtable;", class.name));
        } else {
            self.emit_line("self->vtable = NULL;");
        }

        let iface_names: Vec<String> = self.class_implements.get(&class.name)
            .cloned()
            .unwrap_or_default();
        for iface_name in &iface_names {
            self.emit_line(&format!(
                "self->itable_{} = &_{}_{}_itable;",
                iface_name, class.name, iface_name
            ));
        }
    }

    fn generate_default_ctor(&mut self, class: &ClassDecl) {
        self.emit_line(&format!(
            "{}* {}_ctor({}* self) {{",
            class.name, class.name, class.name
        ));
        self.indent += 1;
        self.emit_vtable_init(class);
        if let Some(parent) = &self.parent_class {
            self.emit_line(&format!("{}_ctor(({}*)self);", parent, parent));
        }
        self.emit_line("return self;");
        self.indent -= 1;
        self.emit_line("}");
        self.emit_line("");
    }

    fn generate_constructor(&mut self, ctor: &ConstructorDecl, class: &ClassDecl) {
        self.emit_line(&format!(
            "{}* {}_ctor({}) {{",
            class.name,
            class.name,
            self.c_params_with_this_ctor(&ctor.params, &class.name)
        ));
        self.indent += 1;

        self.var_types.clear();
        self.var_types.insert("self".to_string(), format!("{}*", class.name));
        for p in &ctor.params {
            self.var_types.insert(p.name.clone(), self.c_type(&p.param_type));
        }
        for member in &class.members {
            if let ClassMember::Field(field) = member {
                self.var_types.insert(field.name.clone(), self.c_type(&field.var_type));
            }
        }

        for stmt in &ctor.body.statements {
            self.generate_stmt(stmt);
        }

        self.emit_vtable_init(class);

        self.emit_line("return self;");
        self.indent -= 1;
        self.emit_line("}");
        self.emit_line("");
    }

    fn generate_method(&mut self, method: &MethodDecl, class: &ClassDecl) {
        let mangled = mangle_method_name(&class.name, &method.name, &method.params);
        self.emit_line(&format!(
            "{} {}({}) {{",
            self.c_type(&method.return_type),
            mangled,
            self.c_params_with_this(&method.params, &class.name)
        ));
        self.indent += 1;

        self.var_types.clear();
        self.var_types.insert("self".to_string(), format!("{}*", class.name));
        for p in &method.params {
            self.var_types.insert(p.name.clone(), self.c_type(&p.param_type));
        }
        // Add class field types for string type detection in expressions
        for member in &class.members {
            if let ClassMember::Field(field) = member {
                self.var_types.insert(field.name.clone(), self.c_type(&field.var_type));
            }
        }

        if let Some(body) = &method.body {
            for stmt in &body.statements {
                self.generate_stmt(stmt);
            }
        }

        self.indent -= 1;
        self.emit_line("}");
        self.emit_line("");
    }

    fn generate_ctor_method(&mut self, method: &MethodDecl, class: &ClassDecl) {
        self.emit_line(&format!(
            "{}* {}_ctor({}) {{",
            class.name,
            class.name,
            self.c_params_with_this_ctor(&method.params, &class.name)
        ));
        self.indent += 1;

        self.var_types.clear();
        self.var_types.insert("self".to_string(), format!("{}*", class.name));
        for p in &method.params {
            self.var_types.insert(p.name.clone(), self.c_type(&p.param_type));
        }
        // Add class field types for string type detection in expressions
        for member in &class.members {
            if let ClassMember::Field(field) = member {
                self.var_types.insert(field.name.clone(), self.c_type(&field.var_type));
            }
        }

        if let Some(body) = &method.body {
            for stmt in &body.statements {
                self.generate_stmt(stmt);
            }
        }

        self.emit_vtable_init(class);

        self.emit_line("return self;");
        self.indent -= 1;
        self.emit_line("}");
        self.emit_line("");
    }

    fn generate_destructor(&mut self, dtor: &DestructorDecl, class: &ClassDecl) {
        self.emit_line(&format!(
            "void {}_dtor({}* self) {{",
            class.name, class.name
        ));
        self.indent += 1;

        self.var_types.clear();
        self.var_types.insert("self".to_string(), format!("{}*", class.name));

        for stmt in &dtor.body.statements {
            self.generate_stmt(stmt);
        }

        self.indent -= 1;
        self.emit_line("}");
        self.emit_line("");
    }

    fn generate_function(&mut self, func: &FunctionDecl) {
        self.emit_line(&format!(
            "{} {}({}) {{",
            self.c_type(&func.return_type),
            func.name,
            self.c_params(&func.params)
        ));
        self.indent += 1;

        self.var_types.clear();
        for p in &func.params {
            self.var_types.insert(p.name.clone(), self.c_type(&p.param_type));
        }

        for stmt in &func.body.statements {
            self.generate_stmt(stmt);
        }

        self.indent -= 1;
        self.emit_line("}");
        self.emit_line("");
    }

    fn generate_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::VarDecl(var) => {
                let mut c_type = self.c_type(&var.var_type);

                if let TypeRef::Named(type_name, _) = &var.var_type {
                    if self.interface_names.contains(type_name) {
                        if let Some(Expr::New(class_name, type_args, _)) = &var.initializer {
                            if !type_args.is_empty() {
                                c_type = format!("{}*", self.mangled_generic_name(class_name, type_args));
                            } else {
                                c_type = format!("{}*", class_name);
                            }
                            self.var_interface_types.insert(var.name.clone(), type_name.clone());
                        } else {
                            c_type = "void*".to_string();
                            self.var_interface_types.insert(var.name.clone(), type_name.clone());
                        }
                    }
                }

                self.var_types.insert(var.name.clone(), c_type.clone());
                let init = match &var.initializer {
                    Some(e) => format!(" = {}", self.gen_expr(e)),
                    None => String::new(),
                };
                self.emit_line(&format!("{} {}{};", c_type, var.name, init));
            }
            Stmt::Return(expr) => {
                match expr {
                    Some(e) => {
                        let val = self.gen_expr(e);
                        self.emit_line(&format!("return {};", val));
                    }
                    None => self.emit_line("return;"),
                }
            }
            Stmt::Expr(expr) => {
                let val = self.gen_expr(expr);
                self.emit_line(&format!("{};", val));
            }
            Stmt::If(cond, then, else_) => {
                let cond_str = self.gen_expr(cond);
                self.emit_line(&format!("if ({}) {{", cond_str));
                self.indent += 1;
                self.generate_stmt(then);
                self.indent -= 1;
                if let Some(else_stmt) = else_ {
                    self.emit_line("} else {");
                    self.indent += 1;
                    self.generate_stmt(else_stmt);
                    self.indent -= 1;
                }
                self.emit_line("}");
            }
            Stmt::While(cond, body) => {
                let cond_str = self.gen_expr(cond);
                self.emit_line(&format!("while ({}) {{", cond_str));
                self.indent += 1;
                self.generate_stmt(body);
                self.indent -= 1;
                self.emit_line("}");
            }
            Stmt::For(init, cond, update, body) => {
                let init_str = match init {
                    Some(stmt) => {
                        match stmt.as_ref() {
                            Stmt::VarDecl(vd) => {
                                let type_str = self.c_type(&vd.var_type);
                                let name = &vd.name;
                                let init_expr = match &vd.initializer {
                                    Some(e) => format!(" = {}", self.gen_expr(e)),
                                    None => String::new(),
                                };
                                format!("{} {}{}", type_str, name, init_expr)
                            }
                            Stmt::Expr(e) => self.gen_expr(e),
                            _ => String::new(),
                        }
                    }
                    None => String::new(),
                };
                let cond_str = match cond {
                    Some(e) => self.gen_expr(e),
                    None => "1".to_string(),
                };
                let update_str = match update {
                    Some(e) => self.gen_expr(e),
                    None => String::new(),
                };
                self.emit_line(&format!("for ({}; {}; {}) {{", init_str, cond_str, update_str));
                self.indent += 1;
                self.generate_stmt(body);
                self.indent -= 1;
                self.emit_line("}");
            }
            Stmt::Block(block) => {
                self.emit_line("{");
                self.indent += 1;
                for s in &block.statements {
                    self.generate_stmt(s);
                }
                self.indent -= 1;
                self.emit_line("}");
            }
            Stmt::Break => self.emit_line("break;"),
            Stmt::Continue => self.emit_line("continue;"),
            Stmt::Switch(subject, cases, default_body) => {
                let subject_str = self.gen_expr(subject);
                self.emit_line(&format!("switch ({}) {{", subject_str));
                self.indent += 1;
                for case in cases {
                    for (i, pattern) in case.patterns.iter().enumerate() {
                        let pattern_str = self.gen_expr(pattern);
                        if i == 0 {
                            self.emit_line(&format!("case {}:", pattern_str));
                        } else {
                            self.emit_line(&format!("case {}:", pattern_str));
                        }
                    }
                    self.indent += 1;
                    for s in &case.body.statements {
                        self.generate_stmt(s);
                    }
                    self.emit_line("break;");
                    self.indent -= 1;
                }
                if let Some(default_block) = default_body {
                    self.emit_line("default:");
                    self.indent += 1;
                    for s in &default_block.statements {
                        self.generate_stmt(s);
                    }
                    self.indent -= 1;
                }
                self.indent -= 1;
                self.emit_line("}");
            }
            Stmt::ForEach(elem_type, name, iterable, body) => {
                let elem_c_type = self.c_type(elem_type);
                let iterable_str = self.gen_expr(iterable);
                self.emit_line(&format!("for (int _fe_i = 0; _fe_i < {}->length; _fe_i++) {{", iterable_str));
                self.indent += 1;
                self.emit_line(&format!("{} {} = ({})LemonArray_get({}, _fe_i);", elem_c_type, name, elem_c_type, iterable_str));
                self.generate_stmt(body);
                self.indent -= 1;
                self.emit_line("}");
            }
            Stmt::Try(try_block, catches, finally) => {
                let exc_id = self.exc_counter;
                self.exc_counter += 1;

                self.emit_line(&format!("LemonException _exc_{};", exc_id));
                self.emit_line(&format!("_exc_{}.type = NULL;", exc_id));
                self.emit_line(&format!("_exc_{}.value = NULL;", exc_id));
                self.emit_line("LemonException* _exc_prev = _exc_stack;");
                self.emit_line(&format!("_exc_stack = &_exc_{};", exc_id));
                self.emit_line(&format!("if (setjmp(_exc_{}.buf) == 0) {{", exc_id));
                self.indent += 1;

                for s in &try_block.statements {
                    self.generate_stmt(s);
                }

                self.emit_line("_exc_stack = _exc_prev;");
                self.indent -= 1;
                self.emit_line("} else {");
                self.indent += 1;

                self.emit_line("_exc_stack = _exc_prev;");

                if catches.is_empty() {
                    if finally.is_some() {
                        for s in &finally.as_ref().unwrap().statements {
                            self.generate_stmt(s);
                        }
                    }
                    self.emit_line(&format!(
                        "if (_exc_{}.type != NULL) {{ lemon_throw(_exc_{}.type, _exc_{}.value); }}",
                        exc_id, exc_id, exc_id
                    ));
                } else {
                    for (i, catch) in catches.iter().enumerate() {
                        let type_name = self.type_ref_name(&catch.var_type);
                        if i == 0 {
                            self.emit_line(&format!(
                                "if (_exc_{}.type != NULL && strcmp(_exc_{}.type, \"{}\") == 0) {{",
                                exc_id, exc_id, type_name
                            ));
                        } else {
                            self.emit_line(&format!(
                                " else if (strcmp(_exc_{}.type, \"{}\") == 0) {{",
                                exc_id, type_name
                            ));
                        }
                        self.indent += 1;

                        let catch_var_type = self.c_type(&catch.var_type);
                        self.var_types.insert(catch.name.clone(), catch_var_type.clone());
                        self.emit_line(&format!(
                            "{}* {} = ({}*)_exc_{}.value;",
                            catch_var_type, catch.name, catch_var_type, exc_id
                        ));

                        for s in &catch.body.statements {
                            self.generate_stmt(s);
                        }

                        self.indent -= 1;
                        self.emit_line("}");
                    }

                    if finally.is_none() {
                        self.emit_line(&format!(
                            " else if (_exc_{}.type != NULL) {{ lemon_throw(_exc_{}.type, _exc_{}.value); }}",
                            exc_id, exc_id, exc_id
                        ));
                    } else {
                        self.emit_line(&format!(
                            " else if (_exc_{}.type != NULL) {{",
                            exc_id
                        ));
                        self.indent += 1;
                        for s in &finally.as_ref().unwrap().statements {
                            self.generate_stmt(s);
                        }
                        self.emit_line(&format!(
                            "lemon_throw(_exc_{}.type, _exc_{}.value);",
                            exc_id, exc_id
                        ));
                        self.indent -= 1;
                        self.emit_line("}");
                    }
                }

                self.indent -= 1;
                self.emit_line("}");

                if let Some(finally_block) = finally {
                    self.emit_line("/* finally on normal path */");
                    for s in &finally_block.statements {
                        self.generate_stmt(s);
                    }
                }
            }
        }
    }

    fn resolve_class_for_var(&self, var_name: &str) -> Option<String> {
        if let Some(ty) = self.var_types.get(var_name) {
            let ty_clean = ty.trim_start_matches("const ");
            if ty_clean.ends_with('*') {
                let class_name = &ty_clean[..ty_clean.len() - 1];
                if !class_name.is_empty() && class_name.chars().next().map_or(false, |c| c.is_uppercase()) {
                    return Some(class_name.to_string());
                }
            }
        }
        if self.method_signatures.contains_key(var_name) {
            return Some(var_name.to_string());
        }
        None
    }

    fn gen_expr(&mut self, expr: &Expr) -> String {
        match expr {
            Expr::IntegerLiteral(v) => format!("{}", v),
            Expr::FloatLiteral(v) => format!("{:.6}", v),
            Expr::StringLiteral(s) => {
                let idx = self.string_counter;
                self.string_counter += 1;
                self.string_literals.push(s.clone());
                format!("_str_{}", idx)
            }
            Expr::CharLiteral(c) => {
                let escaped = match c {
                    '\0' => "\\0".to_string(),
                    '\n' => "\\n".to_string(),
                    '\t' => "\\t".to_string(),
                    '\r' => "\\r".to_string(),
                    '\\' => "\\\\".to_string(),
                    '\'' => "\\'".to_string(),
                    c if (*c as u32) < 32 => format!("\\x{:02x}", *c as u32),
                    c => c.to_string(),
                };
                format!("'{}'", escaped)
            }
            Expr::BoolLiteral(b) => if *b { "1".to_string() } else { "0".to_string() },
            Expr::Null => "NULL".to_string(),
            Expr::This => "self".to_string(),
            Expr::Super => "self".to_string(),
            Expr::Variable(name) => name.clone(),
            Expr::BinaryOp(op, left, right) => {
                let l = self.gen_expr(left);
                let r = self.gen_expr(right);
                if *op == BinaryOp::Add {
                    let left_is_string = self.expr_is_string_type(left);
                    let right_is_string = self.expr_is_string_type(right);
                    if left_is_string || right_is_string {
                        return self.gen_string_concat(left, right, left_is_string, right_is_string);
                    }
                }
                format!("({} {} {})", l, self.c_op(op), r)
            }
            Expr::UnaryOp(op, operand) => {
                let o = self.gen_expr(operand);
                match op {
                    UnaryOp::Plus => format!("(+{})", o),
                    UnaryOp::Minus => format!("(-{})", o),
                    UnaryOp::Not => format!("(!{})", o),
                    UnaryOp::BitNot => format!("(~{})", o),
                    UnaryOp::Deref => format!("(*{})", o),
                    UnaryOp::AddressOf => format!("(&{})", o),
                    UnaryOp::PreInc => format!("(++{})", o),
                    UnaryOp::PreDec => format!("(--{})", o),
                    UnaryOp::PostInc => format!("({}++)", o),
                    UnaryOp::PostDec => format!("({}--)", o),
                }
            }
            Expr::Ternary(cond, then, else_) => {
                let c = self.gen_expr(cond);
                let t = self.gen_expr(then);
                let e = self.gen_expr(else_);
                format!("({} ? {} : {})", c, t, e)
            }
            Expr::Assignment(target, value) => {
                let t = self.gen_expr(target);
                let v = self.gen_expr(value);
                format!("({} = {})", t, v)
            }
            Expr::Call(callee, args) => {
                let arg_strs: Vec<String> = args.iter().map(|a| self.gen_expr(a)).collect();
                match callee.as_ref() {
                    Expr::Variable(name) => {
                        format!("{}({})", name, arg_strs.join(", "))
                    }
                    Expr::FieldAccess(obj, method) => {
                        if let Expr::Variable(var_name) = obj.as_ref() {
                            if var_name == "System" && self.is_system_builtin_method(method) {
                                return format!("{}({})", method, arg_strs.join(", "));
                            }
                            if Self::is_simple_builtin_type(var_name) {
                                return Self::gen_builtin_type_method_call(var_name, method, &arg_strs);
                            }
                        }

                        let obj_str = self.gen_expr(obj);
                        let class_name = self.infer_class_from_expr(obj);
                        let fallback = self.infer_string_method(obj, method);
                        let args_part = if arg_strs.is_empty() { String::new() } else { format!(", {}", arg_strs.join(", ")) };

                        if let Some(iface_name) = self.infer_interface_from_expr(obj) {
                            format!("{}->itable_{}->{}({}{})", obj_str, iface_name, method, obj_str, args_part)
                        } else if let Some(cn) = &class_name {
                            let mangled = self.resolve_method_overload(cn, method, &arg_strs);
                            let is_static = matches!(obj.as_ref(), Expr::Variable(name) if self.method_signatures.contains_key(name.as_str()) && !self.var_types.contains_key(name.as_str()));
                            if is_static {
                                format!("{}(NULL{})", mangled, args_part)
                            } else if self.is_virtual_method(cn, method, arg_strs.len()) && !matches!(obj.as_ref(), Expr::This | Expr::Super) {
                                format!("(({}_vtable*){}->vtable)->{}({}{})", cn, obj_str, mangled, obj_str, args_part)
                            } else {
                                let cast_obj = if cn != self.current_class.as_deref().unwrap_or("") {
                                    format!("({}*){}", cn, obj_str)
                                } else {
                                    obj_str.clone()
                                };
                                format!("{}({}{})", mangled, cast_obj, args_part)
                            }
                        } else if fallback != "void" {
                            format!("{}_{}({}{})", fallback, method, obj_str, args_part)
                        } else {
                            format!("{}_{}({}{})", obj_str, method, obj_str, args_part)
                        }
                    }
                    Expr::Super => {
                        if let Some(parent) = &self.parent_class {
                            let args_part = if arg_strs.is_empty() { String::new() } else { format!(", {}", arg_strs.join(", ")) };
                            format!("{}_ctor(({}*)self{})", parent, parent, args_part)
                        } else if let Some(cn) = &self.current_class {
                            format!("{}_super({})", cn, arg_strs.join(", "))
                        } else {
                            format!("super_call({})", arg_strs.join(", "))
                        }
                    }
                    _ => format!("/* unknown call */({})", arg_strs.join(", "))
                }
            }
            Expr::MethodCall(obj, method, args) => {
                if let Expr::Variable(var_name) = obj.as_ref() {
                    if var_name == "System" && self.is_system_builtin_method(method) {
                        let arg_strs: Vec<String> = args.iter().map(|a| self.gen_expr(a)).collect();
                        return format!("{}({})", method, arg_strs.join(", "));
                    }
                    if Self::is_simple_builtin_type(var_name) {
                        let arg_strs: Vec<String> = args.iter().map(|a| self.gen_expr(a)).collect();
                        return Self::gen_builtin_type_method_call(var_name, method, &arg_strs);
                    }
                }

                let arg_strs: Vec<String> = args.iter().map(|a| self.gen_expr(a)).collect();
                let args_str = if arg_strs.is_empty() { String::new() } else { format!(", {}", arg_strs.join(", ")) };
                let class_name = self.infer_class_from_expr(obj);

                if let Some(iface_name) = self.infer_interface_from_expr(obj) {
                    let obj_str = self.gen_expr(obj);
                    format!("{}->itable_{}->{}({}{})", obj_str, iface_name, method, obj_str, args_str)
                } else if let Some(cn) = &class_name {
                    let mangled = self.resolve_method_overload(cn, method, &arg_strs);
                    if matches!(obj.as_ref(), Expr::Variable(name) if self.method_signatures.contains_key(name)) {
                        format!("{}(NULL{})", mangled, args_str)
                    } else if self.is_virtual_method(cn, method, arg_strs.len()) && !matches!(obj.as_ref(), Expr::This | Expr::Super) {
                        let obj_str = self.gen_expr(obj);
                        format!("(({}_vtable*){}->vtable)->{}({}{})", cn, obj_str, mangled, obj_str, args_str)
                    } else {
                        let obj_str = self.gen_expr(obj);
                        let cast_obj = if cn != self.current_class.as_deref().unwrap_or("") {
                            format!("({}*){}", cn, obj_str)
                        } else {
                            obj_str.clone()
                        };
                        format!("{}({}{})", mangled, cast_obj, args_str)
                    }
                } else {
                    let obj_str = self.gen_expr(obj);
                    let inferred_type = self.infer_string_method(obj, method);
                    format!("{}_{}({}{})", inferred_type, method, obj_str, args_str)
                }
            }
            Expr::FieldAccess(obj, field) => {
                if let Expr::Variable(var_name) = obj.as_ref() {
                    if self.enums.contains_key(var_name) {
                        return format!("{}_KIND_{}", var_name, field);
                    }
                    if Self::is_simple_builtin_type(var_name) {
                        // Array/Map don't have static fields, but String/File etc do
                        if var_name == "Array" || var_name == "Map" {
                            return format!("LemonArray_{}", field);
                        }
                        return format!("{}_{}", var_name, field);
                    }
                    if self.class_fields.contains_key(var_name) {
                        return format!("{}_{}", var_name.to_uppercase(), field);
                    }
                }
                let obj_str = self.gen_expr(obj);
                let is_ptr = self.is_pointer_expr(obj);
                let access = if is_ptr { "->" } else { "." };
                format!("{}{}{}", obj_str, access, field)
            }
            Expr::New(class_name, type_args, args) => {
                let arg_strs: Vec<String> = args.iter().map(|a| self.gen_expr(a)).collect();
                // Built-in types should use their special paths regardless of type_args
                match class_name.as_str() {
                    "Array" => {
                        let elem_size = if arg_strs.is_empty() { "sizeof(void*)" } else { &arg_strs[0] };
                        format!("LemonArray_new({})", elem_size)
                    }
                    "Map" => "LemonMap_new()".to_string(),
                    "StringBuilder" => "StringBuilder_new()".to_string(),
                    _ => {
                        if !type_args.is_empty() {
                            let mangled = self.mangled_generic_name(class_name, type_args);
                            let args_part = if arg_strs.is_empty() { String::new() } else { format!(", {}", arg_strs.join(", ")) };
                            format!("{}_ctor(malloc(sizeof({})){})", mangled, mangled, args_part)
                        } else {
                            let args_part = if arg_strs.is_empty() { String::new() } else { format!(", {}", arg_strs.join(", ")) };
                            format!("{}_ctor(malloc(sizeof({})){})", class_name, class_name, args_part)
                        }
                    }
                }
            }
            Expr::ArrayAccess(arr, idx) => {
                let a = self.gen_expr(arr);
                let i = self.gen_expr(idx);
                format!("{}[{}]", a, i)
            }
            Expr::Cast(target_type, expr) => {
                let e = self.gen_expr(expr);
                format!("(({}) {})", self.c_type(target_type), e)
            }
            Expr::InstanceOf(_expr, _type_ref) => {
                "/* instanceof */ (1)".to_string()
            }
            Expr::Delete(expr) => {
                let e = self.gen_expr(expr);
                format!("free({})", e)
            }
            Expr::Sizeof(_type_ref) => "0".to_string(),
            Expr::TypeId(_expr) => "0".to_string(),
            Expr::Lambda(_, _) => "/* lambda */ NULL".to_string(),
            Expr::Throw(expr) => {
                let val = self.gen_expr(expr);
                let type_name = self.infer_throw_type(expr);
                format!("lemon_throw(\"{}\", (void*){})", type_name, val)
            }
            Expr::Match(subject, arms) => {
                self.gen_match_expr(subject, arms)
            }
        }
    }

    fn is_system_builtin_method(&self, name: &str) -> bool {
        matches!(
            name,
            "printf" | "fprintf" | "sprintf" | "snprintf" | "vsnprintf"
            | "malloc" | "free" | "realloc" | "calloc"
            | "exit" | "abort"
            | "memcpy" | "memset" | "memmove"
            | "strlen" | "strcmp" | "strncmp" | "strdup" | "strstr"
            | "fopen" | "fclose" | "fread" | "fwrite" | "fseek" | "ftell" | "fgets" | "fputs" | "fputc" | "fgetc" | "ungetc" | "feof" | "ferror" | "fflush"
            | "scanf" | "sscanf" | "getchar" | "putchar"
            | "rand" | "srand" | "time" | "clock"
            | "sin" | "cos" | "tan" | "sqrt" | "pow" | "log" | "log10" | "exp" | "fabs" | "ceil" | "floor" | "round" | "fmod"
            | "system" | "getenv" | "getpid" | "getppid"
            | "gc_init" | "gc_mark" | "gc_sweep" | "gc_alloc" | "type_of"
        )
    }

    fn is_simple_builtin_type(name: &str) -> bool {
        matches!(
            name,
            "String" | "StringBuilder" | "Character" | "File" | "Array" | "Map"
        )
    }

    fn gen_builtin_type_method_call(type_name: &str, method: &str, args: &[String]) -> String {
        match type_name {
            "Array" => format!("LemonArray_{}({})", method, args.join(", ")),
            "Map" => format!("LemonMap_{}({})", method, args.join(", ")),
            _ => format!("{}_{}({})", type_name, method, args.join(", ")),
        }
    }

    fn infer_class_from_expr(&self, expr: &Expr) -> Option<String> {
        match expr {
            Expr::This => self.current_class.clone(),
            Expr::Super => self.parent_class.clone().or(self.current_class.clone()),
            Expr::Variable(name) => self.resolve_class_for_var(name),
            Expr::FieldAccess(obj, field) => {
                // Check if the field itself has a known type
                if let Some(ty) = self.var_types.get(field) {
                    if ty == "LemonArray*" {
                        return Some("LemonArray".to_string());
                    }
                    if ty == "LemonMap*" {
                        return Some("LemonMap".to_string());
                    }
                    if ty == "StringBuilder*" {
                        return Some("StringBuilder".to_string());
                    }
                    if ty == "char*" || ty == "const char*" {
                        return Some("String".to_string());
                    }
                    if ty == "LemonFile*" {
                        return Some("LemonFile".to_string());
                    }
                    // Check for class pointer types
                    if ty.ends_with('*') {
                        let class_name = &ty[..ty.len() - 1];
                        if !class_name.is_empty() && !class_name.starts_with("const ") {
                            return Some(class_name.to_string());
                        }
                        let clean = ty.trim_start_matches("const ");
                        if clean.ends_with('*') {
                            let cn = &clean[..clean.len() - 1];
                            if !cn.is_empty() {
                                return Some(cn.to_string());
                            }
                        }
                    }
                }
                self.infer_class_from_expr(obj)
            }
            Expr::New(class_name, type_args, _) => {
                if !type_args.is_empty() {
                    Some(self.mangled_generic_name(class_name, type_args))
                } else {
                    Some(class_name.clone())
                }
            }
            Expr::Cast(target_type, _) => {
                if let TypeRef::Named(name, _) = target_type {
                    Some(name.clone())
                } else {
                    None
                }
            }
            Expr::Call(callee, _args) => {
                // Try to infer the return type of a method call
                if let Expr::FieldAccess(obj, method) = callee.as_ref() {
                    // Check if this is a method that returns a known type
                    if method == "getErrors" || method == "getErrors" {
                        return Some("LemonArray".to_string());
                    }
                    // Check if calling a method on a known builtin type
                    let obj_class = self.infer_class_from_expr(obj);
                    if let Some(cn) = &obj_class {
                        if cn == "LemonArray" {
                            // Array methods that return Array
                            if method == "keys" || method == "values" {
                                return Some("LemonArray".to_string());
                            }
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }

    fn infer_string_method(&self, obj: &Expr, _method: &str) -> String {
        if let Expr::Variable(name) = obj {
            if let Some(ty) = self.var_types.get(name) {
                if ty == "char*" || ty == "const char*" {
                    return "String".to_string();
                }
                if ty == "LemonArray*" {
                    return "LemonArray".to_string();
                }
                if ty == "LemonMap*" {
                    return "LemonMap".to_string();
                }
                if ty == "StringBuilder*" {
                    return "StringBuilder".to_string();
                }
                if ty == "LemonFile*" {
                    return "LemonFile".to_string();
                }
            }
        }
        "void".to_string()
    }

    fn infer_throw_type(&self, expr: &Expr) -> String {
        match expr {
            Expr::New(class_name, type_args, _) => {
                if !type_args.is_empty() {
                    self.mangled_generic_name(class_name, type_args)
                } else {
                    class_name.clone()
                }
            }
            Expr::Variable(name) => {
                if let Some(ty) = self.var_types.get(name) {
                    let ty_clean = ty.trim_start_matches("const ");
                    if ty_clean.ends_with('*') {
                        let type_name = &ty_clean[..ty_clean.len() - 1];
                        if !type_name.is_empty() {
                            return type_name.to_string();
                        }
                    }
                    return ty_clean.to_string();
                }
                "unknown".to_string()
            }
            Expr::Cast(target_type, _) => {
                self.type_ref_name(target_type)
            }
            Expr::This => {
                self.current_class.clone().unwrap_or_else(|| "unknown".to_string())
            }
            _ => "unknown".to_string(),
        }
    }

    fn is_pointer_expr(&self, expr: &Expr) -> bool {
        match expr {
            Expr::This | Expr::Super => true,
            Expr::Variable(name) => {
                if let Some(ty) = self.var_types.get(name) {
                    ty.ends_with('*')
                } else {
                    false
                }
            }
            Expr::New(_, _, _) => true,
            Expr::FieldAccess(_, _) => true,
            Expr::Cast(_, _) => true,
            Expr::Call(_, _) => true,
            _ => false,
        }
    }

    fn c_type(&self, type_ref: &TypeRef) -> String {
        match type_ref {
            TypeRef::Primitive(p) => match p {
                PrimitiveType::Void => "void".to_string(),
                PrimitiveType::Bool => "int".to_string(),
                PrimitiveType::Byte => "int8_t".to_string(),
                PrimitiveType::Char => "char".to_string(),
                PrimitiveType::Short => "int16_t".to_string(),
                PrimitiveType::Int => "int32_t".to_string(),
                PrimitiveType::Long => "int64_t".to_string(),
                PrimitiveType::Float => "float".to_string(),
                PrimitiveType::Double => "double".to_string(),
            },
            TypeRef::Named(name, type_args) => match name.as_str() {
                "int" => "int32_t".to_string(),
                "long" => "int64_t".to_string(),
                "float" => "float".to_string(),
                "double" => "double".to_string(),
                "bool" => "int".to_string(),
                "void" => "void".to_string(),
                "String" => "const char*".to_string(),
                "Array" => "LemonArray*".to_string(),
                "Map" => "LemonMap*".to_string(),
                "StringBuilder" => "StringBuilder*".to_string(),
                "LemonFile" => "LemonFile*".to_string(),
                _ => {
                    if !type_args.is_empty() {
                        format!("{}*", self.mangled_generic_name(name, type_args))
                    } else {
                        format!("{}*", name)
                    }
                }
            },
            TypeRef::Array(inner) => {
                match inner.as_ref() {
                    TypeRef::Named(name, _) if name == "String" => "char**".to_string(),
                    _ => {
                        let inner_c = self.c_type(inner);
                        if inner_c.ends_with('*') {
                            inner_c
                        } else {
                            format!("{}*", inner_c)
                        }
                    }
                }
            }
            TypeRef::FunctionPtr(_, _) => "void*".to_string(),
        }
    }

    fn type_ref_name(&self, type_ref: &TypeRef) -> String {
        match type_ref {
            TypeRef::Primitive(p) => format!("{:?}", p).to_lowercase(),
            TypeRef::Named(name, _) => name.clone(),
            TypeRef::Array(inner) => format!("{}[]", self.type_ref_name(inner)),
            TypeRef::FunctionPtr(_, _) => "function".to_string(),
        }
    }

    fn c_op(&self, op: &BinaryOp) -> &'static str {
        match op {
            BinaryOp::Add => "+",
            BinaryOp::Sub => "-",
            BinaryOp::Mul => "*",
            BinaryOp::Div => "/",
            BinaryOp::Mod => "%",
            BinaryOp::Lt => "<",
            BinaryOp::Gt => ">",
            BinaryOp::Le => "<=",
            BinaryOp::Ge => ">=",
            BinaryOp::Eq => "==",
            BinaryOp::Ne => "!=",
            BinaryOp::And => "&&",
            BinaryOp::Or => "||",
            BinaryOp::BitAnd => "&",
            BinaryOp::BitOr => "|",
            BinaryOp::BitXor => "^",
            BinaryOp::Shl => "<<",
            BinaryOp::Shr => ">>",
        }
    }

    fn c_params(&self, params: &[Param]) -> String {
        if params.is_empty() {
            return "void".to_string();
        }
        params.iter().map(|p| format!("{} {}", self.c_type(&p.param_type), p.name)).collect::<Vec<_>>().join(", ")
    }

    fn c_params_with_this(&self, params: &[Param], class_name: &str) -> String {
        let mut result = vec![format!("{}* self", class_name)];
        for p in params {
            result.push(format!("{} {}", self.c_type(&p.param_type), p.name));
        }
        result.join(", ")
    }

    fn c_params_with_this_ctor(&self, params: &[Param], class_name: &str) -> String {
        let mut result = vec![format!("{}* self", class_name)];
        for p in params {
            result.push(format!("{} {}", self.c_type(&p.param_type), p.name));
        }
        result.join(", ")
    }

    fn c_params_with_void_this(&self, params: &[Param]) -> String {
        let mut result = vec!["void* self".to_string()];
        for p in params {
            result.push(format!("{} {}", self.c_type(&p.param_type), p.name));
        }
        result.join(", ")
    }

    fn collect_interface_info(&mut self, ast: &Program) {
        for decl in &ast.declarations {
            if let Declaration::Interface(iface) = decl {
                self.interface_names.insert(iface.name.clone());
                let mut methods = Vec::new();
                for member in &iface.members {
                    if let InterfaceMember::MethodSignature(sig) = member {
                        methods.push(InterfaceMethodEntry {
                            name: sig.name.clone(),
                            return_type: sig.return_type.clone(),
                            params: sig.params.clone(),
                        });
                    }
                }
                self.interface_methods.insert(iface.name.clone(), methods);
            }
        }

        for decl in &ast.declarations {
            if let Declaration::Class(class) = decl {
                let mut impl_interfaces = Vec::new();
                for iface_type in &class.implements {
                    if let TypeRef::Named(name, _) = iface_type {
                        impl_interfaces.push(name.clone());
                    }
                }
                if !impl_interfaces.is_empty() {
                    self.class_implements.insert(class.name.clone(), impl_interfaces);
                }
            }
        }
    }

    fn infer_interface_from_expr(&self, expr: &Expr) -> Option<String> {
        match expr {
            Expr::Variable(name) => {
                if let Some(iface_name) = self.var_interface_types.get(name) {
                    return Some(iface_name.clone());
                }
                if let Some(ty) = self.var_types.get(name) {
                    let ty_clean = ty.trim_start_matches("const ");
                    if ty_clean.ends_with('*') {
                        let type_name = &ty_clean[..ty_clean.len() - 1];
                        if self.interface_names.contains(type_name) {
                            return Some(type_name.to_string());
                        }
                    }
                }
                None
            }
            Expr::FieldAccess(obj, _) => self.infer_interface_from_expr(obj),
            _ => None,
        }
    }

    fn mangled_type_name(&self, type_ref: &TypeRef) -> String {
        match type_ref {
            TypeRef::Primitive(p) => match p {
                PrimitiveType::Void => "void".to_string(),
                PrimitiveType::Bool => "bool".to_string(),
                PrimitiveType::Byte => "byte".to_string(),
                PrimitiveType::Char => "char".to_string(),
                PrimitiveType::Short => "short".to_string(),
                PrimitiveType::Int => "int".to_string(),
                PrimitiveType::Long => "long".to_string(),
                PrimitiveType::Float => "float".to_string(),
                PrimitiveType::Double => "double".to_string(),
            },
            TypeRef::Named(name, type_args) => {
                if type_args.is_empty() {
                    name.clone()
                } else {
                    self.mangled_generic_name(name, type_args)
                }
            }
            TypeRef::Array(inner) => format!("{}Arr", self.mangled_type_name(inner)),
            TypeRef::FunctionPtr(_, _) => "fnptr".to_string(),
        }
    }

    fn mangled_generic_name(&self, class_name: &str, type_args: &[TypeRef]) -> String {
        let parts: Vec<String> = std::iter::once(class_name.to_string())
            .chain(type_args.iter().map(|ta| self.mangled_type_name(ta)))
            .collect();
        parts.join("_")
    }

    fn substitute_type(&self, type_ref: &TypeRef, mapping: &HashMap<String, TypeRef>) -> TypeRef {
        match type_ref {
            TypeRef::Named(name, type_args) => {
                if let Some(replacement) = mapping.get(name) {
                    replacement.clone()
                } else {
                    let new_args: Vec<TypeRef> = type_args
                        .iter()
                        .map(|ta| self.substitute_type(ta, mapping))
                        .collect();
                    TypeRef::Named(name.clone(), new_args)
                }
            }
            TypeRef::Array(inner) => {
                TypeRef::Array(Box::new(self.substitute_type(inner, mapping)))
            }
            TypeRef::FunctionPtr(ret, params) => {
                TypeRef::FunctionPtr(
                    Box::new(self.substitute_type(ret, mapping)),
                    params.iter().map(|p| self.substitute_type(p, mapping)).collect(),
                )
            }
            TypeRef::Primitive(_) => type_ref.clone(),
        }
    }

    fn collect_generic_instances(&mut self, ast: &Program) {
        for decl in &ast.declarations {
            self.collect_generic_instances_from_decl(decl);
        }
    }

    fn collect_generic_instances_from_decl(&mut self, decl: &Declaration) {
        match decl {
            Declaration::Class(class) => {
                for member in &class.members {
                    match member {
                        ClassMember::Method(method) => {
                            if let Some(body) = &method.body {
                                for stmt in &body.statements {
                                    self.collect_generic_instances_from_stmt(stmt);
                                }
                            }
                        }
                        ClassMember::Constructor(ctor) => {
                            for stmt in &ctor.body.statements {
                                self.collect_generic_instances_from_stmt(stmt);
                            }
                        }
                        ClassMember::Destructor(dtor) => {
                            for stmt in &dtor.body.statements {
                                self.collect_generic_instances_from_stmt(stmt);
                            }
                        }
                        ClassMember::Field(field) => {
                            if let Some(init) = &field.initializer {
                                self.collect_generic_instances_from_expr(init);
                            }
                        }
                    }
                }
            }
            Declaration::Function(func) => {
                for stmt in &func.body.statements {
                    self.collect_generic_instances_from_stmt(stmt);
                }
            }
            _ => {}
        }
    }

    fn collect_generic_instances_from_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::VarDecl(var) => {
                if let Some(init) = &var.initializer {
                    self.collect_generic_instances_from_expr(init);
                }
            }
            Stmt::Return(expr) => {
                if let Some(e) = expr {
                    self.collect_generic_instances_from_expr(e);
                }
            }
            Stmt::Expr(expr) => {
                self.collect_generic_instances_from_expr(expr);
            }
            Stmt::If(cond, then, else_) => {
                self.collect_generic_instances_from_expr(cond);
                self.collect_generic_instances_from_stmt(then);
                if let Some(else_stmt) = else_ {
                    self.collect_generic_instances_from_stmt(else_stmt);
                }
            }
            Stmt::While(cond, body) => {
                self.collect_generic_instances_from_expr(cond);
                self.collect_generic_instances_from_stmt(body);
            }
            Stmt::For(init, cond, update, body) => {
                if let Some(stmt) = init {
                    match stmt.as_ref() {
                        Stmt::VarDecl(vd) => {
                            if let Some(e) = &vd.initializer {
                                self.collect_generic_instances_from_expr(e);
                            }
                        }
                        Stmt::Expr(e) => {
                            self.collect_generic_instances_from_expr(e);
                        }
                        _ => {}
                    }
                }
                if let Some(e) = cond {
                    self.collect_generic_instances_from_expr(e);
                }
                if let Some(e) = update {
                    self.collect_generic_instances_from_expr(e);
                }
                self.collect_generic_instances_from_stmt(body);
            }
            Stmt::Block(block) => {
                for s in &block.statements {
                    self.collect_generic_instances_from_stmt(s);
                }
            }
            Stmt::Try(try_block, catches, finally) => {
                for s in &try_block.statements {
                    self.collect_generic_instances_from_stmt(s);
                }
                for catch in catches {
                    for s in &catch.body.statements {
                        self.collect_generic_instances_from_stmt(s);
                    }
                }
                if let Some(finally_block) = finally {
                    for s in &finally_block.statements {
                        self.collect_generic_instances_from_stmt(s);
                    }
                }
            }
            Stmt::Break | Stmt::Continue => {}
            Stmt::Switch(subject, cases, default_body) => {
                self.collect_generic_instances_from_expr(subject);
                for case in cases {
                    for pattern in &case.patterns {
                        self.collect_generic_instances_from_expr(pattern);
                    }
                    for s in &case.body.statements {
                        self.collect_generic_instances_from_stmt(s);
                    }
                }
                if let Some(default_block) = default_body {
                    for s in &default_block.statements {
                        self.collect_generic_instances_from_stmt(s);
                    }
                }
            }
            Stmt::ForEach(elem_type, _name, iterable, body) => {
                if let TypeRef::Named(name, type_args) = elem_type {
                    if !type_args.is_empty() && self.generic_classes.contains_key(name) {
                        let exists = self.generic_instances.get(name).map_or(false, |entries| {
                            entries.iter().any(|t| t.len() == type_args.len() && t.iter().zip(type_args.iter()).all(|(a, b)| self.types_equal(a, b)))
                        });
                        if !exists {
                            self.generic_instances.entry(name.clone()).or_default().push(type_args.clone());
                        }
                    }
                }
                self.collect_generic_instances_from_expr(iterable);
                self.collect_generic_instances_from_stmt(body);
            }
        }
    }

    fn collect_generic_instances_from_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::New(class_name, type_args, args) => {
                if !type_args.is_empty() && self.generic_classes.contains_key(class_name) {
                    let already_exists = self.generic_instances.get(class_name).map_or(false, |instances| {
                        instances.iter().any(|existing| {
                            existing.len() == type_args.len()
                                && existing.iter().zip(type_args.iter()).all(|(a, b)| self.types_equal(a, b))
                        })
                    });
                    if !already_exists {
                        self.generic_instances.entry(class_name.clone()).or_default().push(type_args.clone());
                    }
                }
                for arg in args {
                    self.collect_generic_instances_from_expr(arg);
                }
            }
            Expr::BinaryOp(_, left, right) => {
                self.collect_generic_instances_from_expr(left);
                self.collect_generic_instances_from_expr(right);
            }
            Expr::UnaryOp(_, operand) => {
                self.collect_generic_instances_from_expr(operand);
            }
            Expr::Assignment(target, value) => {
                self.collect_generic_instances_from_expr(target);
                self.collect_generic_instances_from_expr(value);
            }
            Expr::Call(callee, args) => {
                self.collect_generic_instances_from_expr(callee);
                for arg in args {
                    self.collect_generic_instances_from_expr(arg);
                }
            }
            Expr::MethodCall(obj, _, args) => {
                self.collect_generic_instances_from_expr(obj);
                for arg in args {
                    self.collect_generic_instances_from_expr(arg);
                }
            }
            Expr::FieldAccess(obj, _) => {
                self.collect_generic_instances_from_expr(obj);
            }
            Expr::ArrayAccess(arr, idx) => {
                self.collect_generic_instances_from_expr(arr);
                self.collect_generic_instances_from_expr(idx);
            }
            Expr::Cast(_, e) => {
                self.collect_generic_instances_from_expr(e);
            }
            Expr::InstanceOf(e, _) => {
                self.collect_generic_instances_from_expr(e);
            }
            Expr::Delete(e) => {
                self.collect_generic_instances_from_expr(e);
            }
            Expr::Ternary(cond, then, else_) => {
                self.collect_generic_instances_from_expr(cond);
                self.collect_generic_instances_from_expr(then);
                self.collect_generic_instances_from_expr(else_);
            }
            Expr::Throw(e) => {
                self.collect_generic_instances_from_expr(e);
            }
            Expr::Lambda(_, _) => {}
            Expr::Sizeof(_) => {}
            Expr::TypeId(e) => {
                self.collect_generic_instances_from_expr(e);
            }
            Expr::Match(subject, arms) => {
                self.collect_generic_instances_from_expr(subject);
                for arm in arms {
                    match &arm.body {
                        MatchBody::Expr(e) => self.collect_generic_instances_from_expr(e),
                        MatchBody::Block(b) => {
                            for s in &b.statements {
                                self.collect_generic_instances_from_stmt(s);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn types_equal(&self, a: &TypeRef, b: &TypeRef) -> bool {
        match (a, b) {
            (TypeRef::Primitive(pa), TypeRef::Primitive(pb)) => pa == pb,
            (TypeRef::Named(na, ta_a), TypeRef::Named(nb, ta_b)) => {
                na == nb
                    && ta_a.len() == ta_b.len()
                    && ta_a.iter().zip(ta_b.iter()).all(|(a, b)| self.types_equal(a, b))
            }
            (TypeRef::Array(ia), TypeRef::Array(ib)) => self.types_equal(ia, ib),
            _ => false,
        }
    }

    fn build_type_param_mapping(&self, class: &ClassDecl, type_args: &[TypeRef]) -> HashMap<String, TypeRef> {
        let mut mapping = HashMap::new();
        for (i, param) in class.type_params.iter().enumerate() {
            if i < type_args.len() {
                mapping.insert(param.name.clone(), type_args[i].clone());
            }
        }
        mapping
    }

    fn generate_generic_struct_def(&mut self, class: &ClassDecl, type_args: &[TypeRef]) {
        let mapping = self.build_type_param_mapping(class, type_args);
        let mangled = self.mangled_generic_name(&class.name, type_args);

        self.emit_line(&format!("struct {} {{", mangled));
        self.indent += 1;
        self.emit_line("void** vtable;");

        for member in &class.members {
            if let ClassMember::Field(field) = member {
                let resolved_type = self.substitute_type(&field.var_type, &mapping);
                self.emit_line(&format!("{} {};", self.c_type(&resolved_type), field.name));
            }
        }

        self.indent -= 1;
        self.emit_line("};");
        self.emit_line("");
    }

    fn emit_generic_forward_decls(&mut self, class: &ClassDecl, type_args: &[TypeRef]) {
        let mapping = self.build_type_param_mapping(class, type_args);
        let mangled = self.mangled_generic_name(&class.name, type_args);

        let has_ctor = self.class_has_explicit_ctor(class);
        if !has_ctor {
            self.emit_line(&format!(
                "{}* {}_ctor({}* self);",
                mangled, mangled, mangled
            ));
        }

        for member in &class.members {
            match member {
                ClassMember::Constructor(ctor) => {
                    let resolved_params: Vec<Param> = ctor.params.iter().map(|p| {
                        let resolved = self.substitute_type(&p.param_type, &mapping);
                        Param {
                            param_type: resolved,
                            name: p.name.clone(),
                            default_value: p.default_value.clone(),
                        }
                    }).collect();
                    self.emit_line(&format!(
                        "{}* {}_ctor({});",
                        mangled,
                        mangled,
                        self.c_params_with_this_ctor(&resolved_params, &mangled)
                    ));
                }
                ClassMember::Method(method) => {
                    let resolved_return = self.substitute_type(&method.return_type, &mapping);
                    let resolved_params: Vec<Param> = method.params.iter().map(|p| {
                        let resolved = self.substitute_type(&p.param_type, &mapping);
                        Param {
                            param_type: resolved,
                            name: p.name.clone(),
                            default_value: p.default_value.clone(),
                        }
                    }).collect();
                    if method.name == class.name {
                        self.emit_line(&format!(
                            "{}* {}_ctor({});",
                            mangled,
                            mangled,
                            self.c_params_with_this_ctor(&resolved_params, &mangled)
                        ));
                    } else {
                        let method_mangled = mangle_method_name(&mangled, &method.name, &resolved_params);
                        self.emit_line(&format!(
                            "{} {}({});",
                            self.c_type(&resolved_return),
                            method_mangled,
                            self.c_params_with_this(&resolved_params, &mangled)
                        ));
                    }
                }
                ClassMember::Destructor(_) => {
                    self.emit_line(&format!(
                        "void {}_dtor({}* self);",
                        mangled, mangled
                    ));
                }
                _ => {}
            }
        }
    }

    fn generate_generic_class(&mut self, class: &ClassDecl, type_args: &[TypeRef]) {
        let mapping = self.build_type_param_mapping(class, type_args);
        let mangled = self.mangled_generic_name(&class.name, type_args);

        self.current_class = Some(mangled.clone());
        self.parent_class = None;
        self.var_types.clear();
        self.var_interface_types.clear();

        for member in &class.members {
            if let ClassMember::Field(field) = member {
                let resolved_type = self.substitute_type(&field.var_type, &mapping);
                self.var_types.insert(field.name.clone(), self.c_type(&resolved_type));
            }
        }

        let has_ctor = self.class_has_explicit_ctor(class);
        if !has_ctor {
            self.emit_line(&format!(
                "{}* {}_ctor({}* self) {{",
                mangled, mangled, mangled
            ));
            self.indent += 1;
            self.emit_line("self->vtable = NULL;");
            self.emit_line("return self;");
            self.indent -= 1;
            self.emit_line("}");
            self.emit_line("");
        }

        for member in &class.members {
            match member {
                ClassMember::Constructor(ctor) => {
                    let resolved_params: Vec<Param> = ctor.params.iter().map(|p| {
                        let resolved = self.substitute_type(&p.param_type, &mapping);
                        Param {
                            param_type: resolved,
                            name: p.name.clone(),
                            default_value: p.default_value.clone(),
                        }
                    }).collect();

                    self.emit_line(&format!(
                        "{}* {}_ctor({}) {{",
                        mangled,
                        mangled,
                        self.c_params_with_this_ctor(&resolved_params, &mangled)
                    ));
                    self.indent += 1;

                    self.var_types.clear();
                    self.var_types.insert("self".to_string(), format!("{}*", mangled));
                    for p in &resolved_params {
                        self.var_types.insert(p.name.clone(), self.c_type(&p.param_type));
                    }

                    for stmt in &ctor.body.statements {
                        self.generate_stmt(stmt);
                    }

                    self.emit_line("self->vtable = NULL;");
                    self.emit_line("return self;");
                    self.indent -= 1;
                    self.emit_line("}");
                    self.emit_line("");
                }
                ClassMember::Method(method) => {
                    let resolved_return = self.substitute_type(&method.return_type, &mapping);
                    let resolved_params: Vec<Param> = method.params.iter().map(|p| {
                        let resolved = self.substitute_type(&p.param_type, &mapping);
                        Param {
                            param_type: resolved,
                            name: p.name.clone(),
                            default_value: p.default_value.clone(),
                        }
                    }).collect();

                    if method.name == class.name {
                        self.emit_line(&format!(
                            "{}* {}_ctor({}) {{",
                            mangled,
                            mangled,
                            self.c_params_with_this_ctor(&resolved_params, &mangled)
                        ));
                        self.indent += 1;

                        self.var_types.clear();
                        self.var_types.insert("self".to_string(), format!("{}*", mangled));
                        for p in &resolved_params {
                            self.var_types.insert(p.name.clone(), self.c_type(&p.param_type));
                        }

                        if let Some(body) = &method.body {
                            for stmt in &body.statements {
                                self.generate_stmt(stmt);
                            }
                        }

                        self.emit_line("self->vtable = NULL;");
                        self.emit_line("return self;");
                        self.indent -= 1;
                        self.emit_line("}");
                        self.emit_line("");
                    } else {
                        let method_mangled = mangle_method_name(&mangled, &method.name, &resolved_params);
                        self.emit_line(&format!(
                            "{} {}({}) {{",
                            self.c_type(&resolved_return),
                            method_mangled,
                            self.c_params_with_this(&resolved_params, &mangled)
                        ));
                        self.indent += 1;

                        self.var_types.clear();
                        self.var_types.insert("self".to_string(), format!("{}*", mangled));
                        for p in &resolved_params {
                            self.var_types.insert(p.name.clone(), self.c_type(&p.param_type));
                        }

                        if let Some(body) = &method.body {
                            for stmt in &body.statements {
                                self.generate_stmt(stmt);
                            }
                        }

                        self.indent -= 1;
                        self.emit_line("}");
                        self.emit_line("");
                    }
                }
                ClassMember::Destructor(dtor) => {
                    self.emit_line(&format!(
                        "void {}_dtor({}* self) {{",
                        mangled, mangled
                    ));
                    self.indent += 1;

                    self.var_types.clear();
                    self.var_types.insert("self".to_string(), format!("{}*", mangled));

                    for stmt in &dtor.body.statements {
                        self.generate_stmt(stmt);
                    }

                    self.indent -= 1;
                    self.emit_line("}");
                    self.emit_line("");
                }
                _ => {}
            }
        }

        self.current_class = None;
        self.parent_class = None;
    }

    fn emit_line(&mut self, line: &str) {
        for _ in 0..self.indent {
            self.output.write_all(b"    ").unwrap();
        }
        self.output.write_all(line.as_bytes()).unwrap();
        self.output.write_all(b"\n").unwrap();
    }

    pub fn string_literals(&self) -> &[String] {
        &self.string_literals
    }

    fn collect_method_signatures(&mut self, ast: &Program) {
        for decl in &ast.declarations {
            if let Declaration::Class(class) = decl {
                let mut sigs = Vec::new();
                for member in &class.members {
                    if let ClassMember::Method(m) = member {
                        if m.name != class.name {
                            let param_types: Vec<TypeRef> = m.params.iter().map(|p| p.param_type.clone()).collect();
                            sigs.push((m.name.clone(), param_types));
                        }
                    }
                }
                self.method_signatures.insert(class.name.clone(), sigs);
            }
        }
    }

    fn resolve_method_overload(&self, class_name: &str, method_name: &str, arg_strs: &[String]) -> String {
        if let Some(sigs) = self.method_signatures.get(class_name) {
            let matching: Vec<&(String, Vec<TypeRef>)> = sigs.iter()
                .filter(|(name, _)| name == method_name)
                .collect();
            if matching.len() == 1 {
                let params: Vec<Param> = matching[0].1.iter().map(|pt| Param {
                    param_type: pt.clone(),
                    name: String::new(),
                    default_value: None,
                }).collect();
                return mangle_method_name(class_name, method_name, &params);
            }
            if matching.len() > 1 {
                let best = matching.iter().min_by_key(|(_, param_types)| {
                    let diff = if param_types.len() == arg_strs.len() { 0 } else { (param_types.len() as i32 - arg_strs.len() as i32).abs() as usize };
                    diff
                });
                if let Some((_, param_types)) = best {
                    let params: Vec<Param> = param_types.iter().map(|pt| Param {
                        param_type: pt.clone(),
                        name: String::new(),
                        default_value: None,
                    }).collect();
                    return mangle_method_name(class_name, method_name, &params);
                }
            }
        }
        format!("{}_{}", class_name, method_name)
    }

    fn generate_enum_struct_def(&mut self, enum_decl: &EnumDecl) {
        self.emit_line(&format!("struct {} {{", enum_decl.name));
        self.indent += 1;
        self.emit_line(&format!("int kind;"));

        let has_payload = enum_decl.variants.iter().any(|v| !v.fields.is_empty());
        if has_payload {
            self.emit_line("union {");
            self.indent += 1;
            for variant in &enum_decl.variants {
                if !variant.fields.is_empty() {
                    self.emit_line(&format!("struct {{"));
                    self.indent += 1;
                    for (i, field) in variant.fields.iter().enumerate() {
                        let field_name = field.name.clone().unwrap_or_else(|| format!("f{}", i));
                        self.emit_line(&format!("{} {};", self.c_type(&field.field_type), field_name));
                    }
                    self.indent -= 1;
                    self.emit_line(&format!("}} {}_data;", variant.name));
                }
            }
            self.indent -= 1;
            self.emit_line("} data;");
        }

        self.indent -= 1;
        self.emit_line("};");
        self.emit_line("");

        for (idx, variant) in enum_decl.variants.iter().enumerate() {
            let variant_const = format!("{}_KIND_{}", enum_decl.name, variant.name);
            self.emit_line(&format!("#define {} {}", variant_const, idx));
        }
        self.emit_line("");

        for variant in &enum_decl.variants {
            if variant.fields.is_empty() {
                self.emit_line(&format!("{} {}_{}() {{", enum_decl.name, enum_decl.name, variant.name));
                self.indent += 1;
                self.emit_line(&format!("{} result;", enum_decl.name));
                self.emit_line(&format!("result.kind = {}_KIND_{};", enum_decl.name, variant.name));
                self.emit_line("return result;");
                self.indent -= 1;
                self.emit_line("}");
            } else {
                let params: Vec<String> = variant.fields.iter().enumerate().map(|(i, f)| {
                    let field_name = f.name.clone().unwrap_or_else(|| format!("f{}", i));
                    format!("{} {}", self.c_type(&f.field_type), field_name)
                }).collect();
                self.emit_line(&format!("{} {}_{}({}) {{", enum_decl.name, enum_decl.name, variant.name, params.join(", ")));
                self.indent += 1;
                self.emit_line(&format!("{} result;", enum_decl.name));
                self.emit_line(&format!("result.kind = {}_KIND_{};", enum_decl.name, variant.name));
                for (i, field) in variant.fields.iter().enumerate() {
                    let field_name = field.name.clone().unwrap_or_else(|| format!("f{}", i));
                    self.emit_line(&format!("result.data.{}_data.{} = {};", variant.name, field_name, field_name));
                }
                self.emit_line("return result;");
                self.indent -= 1;
                self.emit_line("}");
            }
            self.emit_line("");
        }
    }

    fn gen_match_expr(&mut self, subject: &Expr, arms: &[MatchArm]) -> String {
        let match_id = self.match_counter;
        self.match_counter += 1;

        let subject_str = self.gen_expr(subject);

        let result_var = format!("_match_result_{}", match_id);
        let subject_var = format!("_match_subject_{}", match_id);

        let enum_name = self.infer_enum_from_expr(subject);
        if let Some(ref en) = enum_name {
            let enum_decl = self.enums.get(en).cloned();
            if let Some(ref enum_decl) = enum_decl {
                self.emit_line(&format!("{} {} = {};", en, subject_var, subject_str));
                self.emit_line(&format!("{} {};", en, result_var));

                let variant_field_map: std::collections::HashMap<String, Vec<(String, String)>> = enum_decl.variants.iter().map(|v| {
                    let fields: Vec<(String, String)> = v.fields.iter().enumerate().map(|(i, f)| {
                        let field_name = f.name.clone().unwrap_or_else(|| format!("f{}", i));
                        (field_name, self.c_type(&f.field_type))
                    }).collect();
                    (v.name.clone(), fields)
                }).collect();

                for (arm_idx, arm) in arms.iter().enumerate() {
                    let keyword = if arm_idx == 0 { "if" } else { "else if" };

                    match &arm.pattern {
                        MatchPattern::Variant(variant_name, bindings) => {
                            self.emit_line(&format!("{} ({}.kind == {}_KIND_{}) {{", keyword, subject_var, en, variant_name));
                            self.indent += 1;
                            for (i, binding) in bindings.iter().enumerate() {
                                let field_name = variant_field_map.get(variant_name)
                                    .and_then(|fields| fields.get(i))
                                    .map(|(n, _)| n.clone())
                                    .unwrap_or_else(|| format!("f{}", i));
                                let binding_c_type = self.c_type(&binding.field_type);
                                self.emit_line(&format!("{} {} = {}.data.{}_data.{};",
                                    binding_c_type, binding.name, subject_var, variant_name, field_name));
                            }
                            match &arm.body {
                                MatchBody::Expr(e) => {
                                    let val = self.gen_expr(e);
                                    self.emit_line(&format!("{} = {};", result_var, val));
                                }
                                MatchBody::Block(b) => {
                                    for s in &b.statements {
                                        self.generate_stmt(s);
                                    }
                                }
                            }
                            self.indent -= 1;
                            self.emit_line("}");
                        }
                        MatchPattern::Wildcard => {
                            self.emit_line("{");
                            self.indent += 1;
                            match &arm.body {
                                MatchBody::Expr(e) => {
                                    let val = self.gen_expr(e);
                                    self.emit_line(&format!("{} = {};", result_var, val));
                                }
                                MatchBody::Block(b) => {
                                    for s in &b.statements {
                                        self.generate_stmt(s);
                                    }
                                }
                            }
                            self.indent -= 1;
                            self.emit_line("}");
                        }
                        MatchPattern::Literal(expr) => {
                            let lit_str = self.gen_expr(expr);
                            self.emit_line(&format!("{} ({}.kind == {}) {{", keyword, subject_var, lit_str));
                            self.indent += 1;
                            match &arm.body {
                                MatchBody::Expr(e) => {
                                    let val = self.gen_expr(e);
                                    self.emit_line(&format!("{} = {};", result_var, val));
                                }
                                MatchBody::Block(b) => {
                                    for s in &b.statements {
                                        self.generate_stmt(s);
                                    }
                                }
                            }
                            self.indent -= 1;
                            self.emit_line("}");
                        }
                        MatchPattern::Or(patterns) => {
                            let en_clone = en.clone();
                            let sv_clone = subject_var.clone();
                            let conditions: Vec<String> = patterns.iter().filter_map(|p| {
                                match p {
                                    MatchPattern::Variant(vn, _) => Some(format!("{}.kind == {}_KIND_{}", sv_clone, en_clone, vn)),
                                    MatchPattern::Wildcard => Some("1".to_string()),
                                    _ => None,
                                }
                            }).collect();
                            self.emit_line(&format!("{} ({}) {{", keyword, conditions.join(" || ")));
                            self.indent += 1;
                            for p in patterns {
                                if let MatchPattern::Variant(vn, bindings) = p {
                                    if !bindings.is_empty() {
                                        self.emit_line(&format!("if ({}.kind == {}_KIND_{}) {{", subject_var, en, vn));
                                        self.indent += 1;
                                        for (i, binding) in bindings.iter().enumerate() {
                                            let field_name = variant_field_map.get(vn.as_str())
                                                .and_then(|fields| fields.get(i))
                                                .map(|(n, _)| n.clone())
                                                .unwrap_or_else(|| format!("f{}", i));
                                            let binding_c_type = self.c_type(&binding.field_type);
                                            self.emit_line(&format!("{} {} = {}.data.{}_data.{};",
                                                binding_c_type, binding.name, subject_var, vn, field_name));
                                        }
                                        self.indent -= 1;
                                        self.emit_line("}");
                                    }
                                }
                            }
                            match &arm.body {
                                MatchBody::Expr(e) => {
                                    let val = self.gen_expr(e);
                                    self.emit_line(&format!("{} = {};", result_var, val));
                                }
                                MatchBody::Block(b) => {
                                    for s in &b.statements {
                                        self.generate_stmt(s);
                                    }
                                }
                            }
                            self.indent -= 1;
                            self.emit_line("}");
                        }
                    }
                }

                return result_var;
            }
        }

        format!("/* unhandled match */ 0")
    }

    fn infer_enum_from_expr(&self, expr: &Expr) -> Option<String> {
        match expr {
            Expr::Variable(name) => {
                if let Some(ty) = self.var_types.get(name) {
                    let ty_clean = ty.trim_start_matches("const ");
                    if self.enums.contains_key(ty_clean) {
                        return Some(ty_clean.to_string());
                    }
                }
                if self.enums.contains_key(name) {
                    return Some(name.clone());
                }
                None
            }
            Expr::FieldAccess(obj, _field) => self.infer_enum_from_expr(obj),
            Expr::MethodCall(obj, _, _) => self.infer_enum_from_expr(obj),
            _ => None,
        }
    }

    fn expr_is_string_type(&self, expr: &Expr) -> bool {
        match expr {
            Expr::StringLiteral(_) => true,
            Expr::Variable(name) => {
                if let Some(ty) = self.var_types.get(name) {
                    ty.contains("char*") || ty.contains("const char*")
                } else {
                    false
                }
            }
            Expr::BinaryOp(BinaryOp::Add, left, _) => {
                self.expr_is_string_type(left)
            }
            Expr::MethodCall(obj, method, _) => {
                let is_string_method = matches!(method.as_str(),
                    "substring" | "toUpperCase" | "toLowerCase" | "trim" | "replace" | "concat" | "intToString" | "longToString" | "doubleToString" | "fromChar" | "fromCharArray" | "join"
                );
                if is_string_method {
                    return true;
                }
                self.expr_is_string_type(obj)
            }
            Expr::FieldAccess(obj, field) => {
                // Check if the field itself is a string type
                if let Some(ty) = self.var_types.get(field) {
                    if ty.contains("char*") || ty.contains("const char*") {
                        return true;
                    }
                }
                self.expr_is_string_type(obj)
            }
            Expr::Call(callee, _) => {
                if let Expr::FieldAccess(obj, method) = callee.as_ref() {
                    if method == "intToString" || method == "longToString" || method == "doubleToString" || method == "toString" || method == "fromChar" || method == "fromCharArray" || method == "join" {
                        return true;
                    }
                    let _ = obj;
                }
                false
            }
            _ => false,
        }
    }

    fn gen_string_concat(&mut self, left: &Expr, right: &Expr, left_is_string: bool, right_is_string: bool) -> String {
        let l = self.gen_expr(left);
        let r = self.gen_expr(right);

        if left_is_string && right_is_string {
            format!("String_concat({}, {})", l, r)
        } else if left_is_string && !right_is_string {
            format!("String_concat({}, {})", l, self.to_string_expr(right, r))
        } else if !left_is_string && right_is_string {
            format!("String_concat({}, {})", self.to_string_expr(left, l), r)
        } else {
            format!("String_concat({}, {})", l, r)
        }
    }

    fn to_string_expr(&self, expr: &Expr, c_expr: String) -> String {
        match expr {
            Expr::IntegerLiteral(_) => format!("String_longToString((int64_t){})", c_expr),
            Expr::FloatLiteral(_) => format!("String_doubleToString((double){})", c_expr),
            Expr::BoolLiteral(_) => format!("String_intToString((int32_t){})", c_expr),
            Expr::Variable(name) => {
                if let Some(ty) = self.var_types.get(name) {
                    if ty == "int64_t" || ty == "long" {
                        format!("String_longToString((int64_t){})", c_expr)
                    } else if ty == "double" || ty == "float" {
                        format!("String_doubleToString((double){})", c_expr)
                    } else {
                        format!("String_intToString((int32_t){})", c_expr)
                    }
                } else {
                    format!("String_intToString((int32_t){})", c_expr)
                }
            }
            _ => format!("String_intToString((int32_t){})", c_expr),
        }
    }
}
