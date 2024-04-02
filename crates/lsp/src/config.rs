#[derive(Default, Clone)]
pub struct Config {
    data: ConfigData,
}

#[derive(Default, Clone)]
pub struct ConfigData {
    database_url: Option<String>,
    num_threads: Option<usize>,
}

impl Config {
    pub fn main_loop_num_threads(&self) -> usize {
        self.data.num_threads.unwrap_or(num_cpus::get_physical())
    }
}
