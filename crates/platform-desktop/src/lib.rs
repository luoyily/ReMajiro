pub mod audio;
pub mod input;
pub mod movie;
pub mod shell;
pub fn install() {
    audio::install();
    movie::install();
    shell::install();
}
