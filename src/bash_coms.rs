use std::io::{BufRead, BufReader, BufWriter, Write, Read};
use std::fs::File;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum BashReq {
    Complete,
    Which,
    SetCmd,
    Ping,
}

pub struct BashClient {
    request_writer: BufWriter<File>,
    response_reader: BufReader<File>,

    cache: std::collections::HashMap<(BashReq, String), Option<String>>,
}

impl BashClient {
    pub fn new(request_pipe: PathBuf, response_pipe: PathBuf) -> std::io::Result<Self> {
        log::debug!("Initializing BashClient with request_pipe: {:?}, response_pipe: {:?}", request_pipe, response_pipe);
        let request_file = std::fs::OpenOptions::new()
            .write(true)
            .open(&request_pipe)?;

        log::debug!("Opened request pipe: {:?}", request_pipe);

        let response_file = std::fs::File::open(&response_pipe)?;

        log::debug!("BashClient connected to pipes: {:?}, {:?}", request_pipe, response_pipe);

        Ok(BashClient {
            request_writer: BufWriter::new(request_file),
            response_reader: BufReader::new(response_file),
            cache: std::collections::HashMap::new(),
        })
    }

    pub fn test_connection(&mut self) {
        // log::debug!("Testing BashClient connection...");
        // self.request_writer.write_all(b"PING\n").unwrap();
        // log::debug!("Sent PING");
        // self.request_writer.flush().unwrap();
        // log::debug!("Flushed request_writer");

        // let mut response = Vec::new();
        // self.response_reader.read_until(b'\0', &mut response).unwrap();
        // log::info!("BashClient test_connection response: {}", String::from_utf8_lossy(&response));

        log::debug!("Testing BashClient connection...");
        self.get_request_uncached(BashReq::Ping, "").unwrap();
    }

    pub fn get_request(&mut self, req_type: BashReq, argument: &str) -> Option<String> {
        if let Some(cached_response) = self.cache.get(&(req_type.clone(), argument.to_string())) {
            // log::debug!("Cache hit for {:?} with argument '{}' res={:?}", req_type, argument, cached_response);
            return cached_response.clone();
        }

        // TODO: do we want to retry?
        let mut response = match self.get_request_uncached(req_type.clone(), argument) {
            Ok(resp) => if resp.is_empty() {
                log::warn!("Received empty response for {:?} with argument '{}'", req_type, argument);
                None
            } else {
                log::debug!("not empty response for {:?} with argument '{}'", resp, argument);
                Some(resp)
            },
            Err(e) => {
                log::error!("Failed to get request for {:?} with argument '{}': {}", req_type, argument, e);
                None
            }
        };
        // log::debug!("Cache miss for {:?} with argument '{}' res={:?}", req_type, argument, response);

        // if  Some("".to_string()) == response {
        //     response = None;
        // }


        self.cache
            .insert((req_type, argument.to_string()), response.clone());
        response
    }

    // TODO: make async?
    fn get_request_uncached(
        &mut self,
        req_type: BashReq,
        argument: &str,
    ) -> std::io::Result<String> {
        let request_line = match req_type {
            BashReq::Complete => format!("COMPLETE {}\n", argument),
            BashReq::Which => format!("WHICH {}\n", argument),
            BashReq::SetCmd => format!("SETCMD {}\n", argument),
            BashReq::Ping => format!("PING {}\n", argument),
        };

        log::debug!("Sending request: '{}'", request_line.replace("\n", "\\n"));
        // log::debug!("Sending request: {:02x?}", request_line.as_bytes());

        self.request_writer.write_all(request_line.as_bytes())?;
        self.request_writer.flush()?;

        let mut response_len = Vec::new();

        // log::debug!("Waiting for response for argument '{}'", argument);
        self.response_reader.read_until(b'\0', &mut response_len)?;
        // remove the trailing null byte
        response_len.retain(|&x| x != b'\0');

        let response = String::from_utf8_lossy(&response_len).to_string();

        log::debug!("Received response: '{}' for argument '{}'", response, argument);

        Ok(response)

    }
}
