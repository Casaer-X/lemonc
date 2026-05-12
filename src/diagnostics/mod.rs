pub fn report_error(line: usize, col: usize, message: &str) {
    eprintln!("[Error] {}:{} - {}", line, col, message);
}

pub fn report_warning(line: usize, col: usize, message: &str) {
    eprintln!("[Warning] {}:{} - {}", line, col, message);
}
