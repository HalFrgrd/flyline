use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum BashReq {
    Complete,
    Which,
}

pub struct BashClient {
    request_writer: BufWriter<File>,
    response_reader: BufReader<File>,

    cache: std::collections::HashMap<(BashReq, String), Option<String>>,
}

impl BashClient {
    pub fn new(request_pipe: PathBuf, response_pipe: PathBuf) -> std::io::Result<Self> {
        let request_file = std::fs::OpenOptions::new()
            .write(true)
            .open(&request_pipe)?;

        let response_file = std::fs::File::open(&response_pipe)?;

        Ok(BashClient {
            request_writer: BufWriter::new(request_file),
            response_reader: BufReader::new(response_file),
            cache: std::collections::HashMap::new(),
        })
    }

    pub fn get_request(&mut self, req_type: BashReq, argument: &str) -> Option<String> {
        if let Some(cached_response) = self.cache.get(&(req_type.clone(), argument.to_string())) {
            log::debug!("Cache hit for {:?} with argument '{}'", req_type, argument);
            return cached_response.clone();
        }

        // TODO: do we want to retry?
        let response = match self.get_request_uncached(req_type.clone(), argument) {
            Ok(resp) if !resp.is_empty() => Some(resp),
            _ => None,
        };

        self.cache
            .insert((req_type, argument.to_string()), response.clone());
        response
    }

    fn get_request_uncached(
        &mut self,
        req_type: BashReq,
        argument: &str,
    ) -> std::io::Result<String> {
        let request_line = match req_type {
            BashReq::Complete => format!("COMPLETE {}\n", argument),
            BashReq::Which => format!("WHICH {}\n", argument),
        };

        self.request_writer.write_all(request_line.as_bytes())?;
        self.request_writer.flush()?;

        let mut response_line = String::new();
        self.response_reader.read_line(&mut response_line)?;

        Ok(response_line.trim_end().to_string())
    }
}
