use std::sync::LazyLock;

static ALL_HOSTNAMES: LazyLock<Vec<String>> = LazyLock::new(|| {
    let mut hostnames: Vec<String> = Vec::new();
    let mut seen_hostnames: std::collections::HashSet<String> = std::collections::HashSet::new();

    let mut add_hostname = |hostname: String| {
        if !hostname.is_empty() && seen_hostnames.insert(hostname.clone()) {
            hostnames.push(hostname);
        }
    };

    if cfg!(test) {
        add_hostname("localhost".to_string());
        add_hostname("server1.example.com".to_string());
        add_hostname("web-prod-01".to_string());
        return hostnames;
    }

    // Parse /etc/hosts
    if let Ok(contents) = std::fs::read_to_string("/etc/hosts") {
        for line in contents.lines() {
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let mut fields = line.split_whitespace();
            // Skip the IP address
            fields.next();
            for hostname in fields {
                add_hostname(hostname.to_string());
            }
        }
    }

    // Parse ~/.ssh/config
    let ssh_config_path = std::env::var("HOME")
        .map(|home| format!("{}/.ssh/config", home))
        .unwrap_or_default();
    if !ssh_config_path.is_empty() {
        if let Ok(contents) = std::fs::read_to_string(ssh_config_path) {
            for line in contents.lines() {
                let line = line.trim();
                if line.starts_with("Host ") {
                    let mut fields = line.split_whitespace();
                    fields.next(); // Skip "Host"
                    for hostname in fields {
                        if hostname != "*" && !hostname.contains('?') && !hostname.contains('*') {
                            add_hostname(hostname.to_string());
                        }
                    }
                }
            }
        }
    }

    // Parse ~/.ssh/known_hosts
    let known_hosts_path = std::env::var("HOME")
        .map(|home| format!("{}/.ssh/known_hosts", home))
        .unwrap_or_default();
    if !known_hosts_path.is_empty() {
        if let Ok(contents) = std::fs::read_to_string(known_hosts_path) {
            for line in contents.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                let mut fields = line.split_whitespace();
                if let Some(host_field) = fields.next() {
                    for hostname in host_field.split(',') {
                        if !hostname.is_empty() && !hostname.starts_with('|') {
                            // skip hashed hosts
                            add_hostname(hostname.to_string());
                        }
                    }
                }
            }
        }
    }

    hostnames.sort();
    hostnames
});

pub fn get_all_hostnames() -> &'static [String] {
    &ALL_HOSTNAMES
}
