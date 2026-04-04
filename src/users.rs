use std::path::Path;
use std::sync::LazyLock;

#[derive(Debug, Clone)]
pub struct User {
    pub username: String,
    pub home_dir: String,
}

static ALL_USERS: LazyLock<Vec<User>> = LazyLock::new(|| {
    let mut users: Vec<User> = Vec::new();
    let mut seen_usernames: std::collections::HashSet<String> = std::collections::HashSet::new();

    let mut add_user = |username: String, home_dir: String| {
        if seen_usernames.insert(username.clone()) {
            users.push(User { username, home_dir });
        }
    };

    // 1. Parse /etc/passwd
    if let Ok(contents) = std::fs::read_to_string("/etc/passwd") {
        for line in contents.lines() {
            let fields: Vec<&str> = line.splitn(7, ':').collect();
            // Format: username:password:uid:gid:gecos:home_dir:shell
            if fields.len() >= 6 {
                let username = fields[0].to_string();
                let home_dir = fields[5].to_string();
                if !username.is_empty() && !home_dir.is_empty() {
                    add_user(username, home_dir);
                }
            }
        }
    }

    // 2. Scan /home/
    if let Ok(entries) = std::fs::read_dir("/home") {
        for entry in entries.flatten() {
            if entry.file_type().is_ok_and(|ft| ft.is_dir()) {
                let name = entry.file_name().to_string_lossy().into_owned();
                let home_dir = format!("/home/{}", name);
                add_user(name, home_dir);
            }
        }
    }

    // 3. Scan /Users/ (macOS default home directory location)
    if let Ok(entries) = std::fs::read_dir("/Users") {
        for entry in entries.flatten() {
            if entry.file_type().is_ok_and(|ft| ft.is_dir()) {
                let name = entry.file_name().to_string_lossy().into_owned();
                // Skip macOS system directories like Shared
                if name == "Shared" {
                    continue;
                }
                let home_dir = format!("/Users/{}", name);
                add_user(name, home_dir);
            }
        }
    }

    // Ensure root is included if /root exists and wasn't already added
    if Path::new("/root").is_dir() {
        add_user("root".to_string(), "/root".to_string());
    }

    users
});

pub fn get_all_users() -> &'static [User] {
    &ALL_USERS
}
