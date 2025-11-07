use std::io::{BufRead, BufReader, BufWriter, Write, Read, Stdout, Stdin};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum BashReq {
    Complete,
    Which,
    SetCmd,
}

pub struct BashClient {
    request_writer: BufWriter<Stdout>,
    response_reader: BufReader<Stdin>,

    cache: std::collections::HashMap<(BashReq, String), Option<String>>,
}

impl BashClient {
    pub fn new(request_pipe: Stdout, response_pipe: Stdin) -> std::io::Result<Self> {
        Ok(BashClient {
            request_writer: BufWriter::new(request_pipe),
            response_reader: BufReader::new(response_pipe),
            cache: std::collections::HashMap::new(),
        })
    }

    pub fn get_request(&mut self, req_type: BashReq, argument: &str) -> Option<String> {
        if let Some(cached_response) = self.cache.get(&(req_type.clone(), argument.to_string())) {
            // log::debug!("Cache hit for {:?} with argument '{}' res={:?}", req_type, argument, cached_response);
            return cached_response.clone();
        }

        // TODO: do we want to retry?
        let mut response = match self.get_request_uncached(req_type.clone(), argument) {
            Ok(resp) => Some(resp),
            Err(e) => {
                log::error!("Failed to get request for {:?} with argument '{}': {}", req_type, argument, e);
                None
            }
        };
        log::debug!("Cache miss for {:?} with argument '{}' res={:?}", req_type, argument, response);

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
        };

        self.request_writer.write_all(request_line.as_bytes())?;
        self.request_writer.flush()?;

        let mut response_len = String::new();
        self.response_reader.read_line(&mut response_len)?;

        log::debug!("Received response length line: '{}' for argument '{}'", response_len.trim(), argument);

        if response_len.starts_with("RESP_LEN=") {
            log::debug!("Received response length line: {}", response_len.trim());
            let len_str = response_len.trim_start_matches("RESP_LEN=").trim();
            if let Ok(len) = len_str.parse::<usize>() {
                const RESP_BODY_PREFIX: &str = "RESP_BODY=";
                let mut response_buf = vec![0u8; len+RESP_BODY_PREFIX.len()];
                self.response_reader.read_exact(&mut response_buf)?;
                let response_line = String::from_utf8_lossy(&response_buf).to_string();
                if response_line.starts_with(RESP_BODY_PREFIX) {
                    let body = response_line[RESP_BODY_PREFIX.len()..].to_string();
                    return Ok(body);
                } else {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "Invalid response body prefix",
                    ));
                }
            } else {
                Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Invalid response length",
                ))
            }
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Missing response length",
            ))
        }

    }
}
