use crate::{Arg, Command};
use regex::Regex;

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedOption {
    short: Option<String>,
    long: Option<String>,
    value_name: Option<String>,
    num_args: Option<String>,
}

fn remove_device_controls(data: &str) -> String {
    Regex::new(r"\\[XZ]'[^']*'")
        .unwrap()
        .replace_all(data, "")
        .into_owned()
}

fn replace_special_escapes(data: &str) -> String {
    data.replace(r"\(oq", "'")
        .replace(r"\(cq", "'")
        .replace(r"\(aq", "'")
        .replace(r"\(dq", "\"")
        .replace(r"\(lq", "\"")
        .replace(r"\(rq", "\"")
        .replace(r"\(em", "--")
        .replace(r"\(en", "-")
        .replace(r"\(mi", "-")
        .replace(r"\(hy", "-")
        .replace(r"\e", "\\")
        .replace(r"\-", "-")
        .replace(r"\&", "")
        .replace(r"\,", "")
        .replace(r"\/", "")
        .replace(r"\^", "")
        .replace(r"\c", "")
        .replace(r"\ ", " ")
        .replace(r"\~", " ")
        .replace(r"\:", "")
        .replace(r"\|", "")
        .replace(r"\%", "")
}

fn strip_font_escapes(data: &str) -> String {
    Regex::new(r"\\f(\([^)]{2}|\[[^\]]*\]|.)")
        .unwrap()
        .replace_all(data, "")
        .into_owned()
}

fn strip_line_comment(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    if trimmed.starts_with(".\\\"") || trimmed.starts_with(".\"") {
        None
    } else {
        Some(line.to_string())
    }
}

fn trim_known_inline_macros(line: &str) -> String {
    let trimmed = line.trim();
    let mut line = trimmed.to_string();

    if let Some(stripped) = trimmed.strip_prefix('.') {
        line = stripped.to_string();

        let macro_re = Regex::new(
            r"^(?:[A-Z][A-Za-z]?|rb|Nm|Fl|Ar|Pa|Ev|Dv|Cm|Ic|No|Sq|Dq|Pq|Em|Sy|Li|Tn|Ns|Op|Oo|Oc|Xo|Xc|Xr)\s+",
        )
        .unwrap();
        while macro_re.is_match(&line) {
            line = macro_re.replace(&line, "").into_owned();
        }
    }

    if line.ends_with(" ,") || line.ends_with(" .") {
        let punctuation = line.chars().last().unwrap();
        line.truncate(line.len() - 2);
        line.push(punctuation);
    }

    line
}

fn normalize_whitespace(data: &str) -> String {
    Regex::new(r"\s+")
        .unwrap()
        .replace_all(data.trim(), " ")
        .into_owned()
}

fn clean_sentence(desc: &str) -> String {
    let desc = normalize_whitespace(desc);
    if desc.is_empty() {
        return desc;
    }

    let max_len = 160;
    let mut sentences = desc
        .split('.')
        .map(str::trim)
        .filter(|segment| !segment.is_empty());
    let mut out = String::new();

    for sentence in sentences.by_ref() {
        let candidate = if out.is_empty() {
            format!("{sentence}.")
        } else {
            format!("{out} {sentence}.")
        };
        if candidate.len() > max_len && !out.is_empty() {
            break;
        }
        out = candidate;
        if out.len() >= max_len {
            break;
        }
    }

    if out.is_empty() {
        desc.chars().take(max_len).collect::<String>()
    } else {
        out.trim_end_matches('.').to_string()
    }
}

fn strip_groff_wrappers(data: &str) -> String {
    let data = remove_device_controls(data);
    let data = strip_font_escapes(&data);
    let data = replace_special_escapes(&data);
    let data = data.replace("\x0C", " ");
    let data = Regex::new(r"(?m)^\.PD(?: \d+)?$")
        .unwrap()
        .replace_all(&data, "")
        .into_owned();
    Regex::new(r"\.([A-Z][A-Za-z]?|rb)\b")
        .unwrap()
        .replace_all(&data, "")
        .into_owned()
}

fn normalize_text(data: &str, cmd_name: &str) -> String {
    let mut lines = Vec::new();

    for raw_line in data.lines() {
        let Some(raw_line) = strip_line_comment(raw_line) else {
            continue;
        };
        let line = raw_line.replace(".Nm", cmd_name);
        let line = strip_groff_wrappers(&line);
        let line = trim_known_inline_macros(&line);
        let line = line
            .replace("\\-\\^-", "--")
            .replace("\\^-", "-")
            .replace("\\^", "")
            .replace(" Ns ", "")
            .replace(" Xo", "")
            .replace(" Xc", "")
            .replace(" Oo ", "[")
            .replace(" Oc", "]")
            .replace(" Op ", "[")
            .replace(" Ar ", " ")
            .replace(" Pa ", " ")
            .replace(" Ev ", " ")
            .replace(" Dv ", " ")
            .replace(" Cm ", " ")
            .replace(" Ic ", " ")
            .replace(" Fl Fl ", " --")
            .replace(" Fl ", " -")
            .replace("No ", "")
            .replace("Sq ", "")
            .replace("Dq ", "")
            .replace("Pq ", "")
            .replace("Em ", "")
            .replace("Sy ", "")
            .replace("Li ", "")
            .replace("Tn ", "")
            .replace("Ux", "Unix")
            .replace("Bx", "BSD");
        let line = normalize_whitespace(&line);
        if !line.is_empty() {
            lines.push(line);
        }
    }

    lines.join("\n")
}

fn unquote(data: &str) -> String {
    let trimmed = data.trim();
    if trimmed.len() >= 2 {
        if (trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('`') && trimmed.ends_with('\''))
        {
            return trimmed[1..trimmed.len() - 1].trim().to_string();
        }
    }
    trimmed.to_string()
}

fn clean_option_source(data: &str, cmd_name: &str) -> String {
    normalize_text(data, cmd_name)
        .replace('\n', ", ")
        .replace('"', "")
        .replace(" [ ", "[")
        .replace(" ]", "]")
        .replace(" ,", ",")
        .replace(" :", ":")
        .replace(" =", "=")
        .replace("= ", "=")
        .replace(" / ", "/")
}

fn split_aliases(option_text: &str) -> Vec<String> {
    Regex::new(r"\s*(?:,|\||/|\bor\b)\s*")
        .unwrap()
        .split(option_text)
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn normalize_value_token(token: &str) -> Option<String> {
    let token = token.trim_matches(|ch: char| ch.is_whitespace() || matches!(ch, ',' | ';' | '.'));
    if token.is_empty() {
        return None;
    }
    let token = token.trim_matches(|ch| matches!(ch, '[' | ']'));
    let token = token.trim();
    if token.is_empty() || token.starts_with('-') {
        return None;
    }
    Some(token.to_string())
}

fn find_value_type(remainder: &str) -> (Option<String>, Option<String>) {
    let remainder = remainder.trim();
    if remainder.is_empty() {
        return (None, None);
    }

    if let Some(caps) = Regex::new(r"^\[=?(?P<value>[^\]]+)\]")
        .unwrap()
        .captures(remainder)
    {
        let value = normalize_value_token(caps.name("value").unwrap().as_str());
        return (value, Some("?".to_string()));
    }

    if let Some(value) = remainder.strip_prefix('=') {
        let value = value
            .split_whitespace()
            .next()
            .and_then(normalize_value_token);
        return (value, Some("1".to_string()));
    }

    let candidate = remainder
        .split_whitespace()
        .next()
        .and_then(normalize_value_token);
    if let Some(value) = candidate {
        if value.chars().all(|ch| ch.is_ascii_digit()) {
            return (None, None);
        }
        return (Some(value), Some("1".to_string()));
    }

    (None, None)
}

fn parse_alias(alias: &str) -> Option<ParsedOption> {
    let alias = alias.trim();
    // Support options that contain brackets for negation, e.g. --[no-]color or --[no]color.
    let caps = Regex::new(r"^(?P<option>--?(?:\[no\-?\])?[A-Za-z0-9#][A-Za-z0-9_-]*)(?P<rest>.*)$")
        .unwrap()
        .captures(alias)?;
    let option = caps.name("option").unwrap().as_str();
    if option == "-" || option == "--" {
        return None;
    }

    let rest = caps.name("rest").map(|m| m.as_str()).unwrap_or("");
    let (value_name, num_args) = find_value_type(rest);
    let mut parsed = ParsedOption {
        short: None,
        long: None,
        value_name,
        num_args,
    };

    if option.starts_with("--") {
        parsed.long = Some(option.to_string());
    } else if option.len() == 2 {
        parsed.short = Some(option.to_string());
    } else {
        parsed.long = Some(format!("--{}", &option[1..]));
    }

    Some(parsed)
}

fn parse_option_declaration(option_text: &str, cmd_name: &str) -> Vec<ParsedOption> {
    let option_text = clean_option_source(option_text, cmd_name);
    let option_text = unquote(&option_text);
    let aliases = split_aliases(&option_text);
    let mut parsed = Vec::new();
    let mut pending_short: Option<ParsedOption> = None;

    for alias in aliases {
        let Some(current) = parse_alias(&alias) else {
            continue;
        };

        match (&pending_short, &current.short, &current.long) {
            (Some(existing), None, Some(_)) if existing.long.is_none() => {
                let mut merged = existing.clone();
                merged.long = current.long.clone();
                if current.value_name.is_some() {
                    merged.value_name = current.value_name.clone();
                    merged.num_args = current.num_args.clone();
                } else if merged.value_name.is_none() {
                    merged.value_name = current.value_name.clone();
                    merged.num_args = current.num_args.clone();
                }
                parsed.push(merged);
                pending_short = None;
            }
            (Some(existing), Some(_), None) => {
                parsed.push(existing.clone());
                pending_short = Some(current);
            }
            _ if current.short.is_some() && current.long.is_none() => {
                if let Some(existing) = pending_short.replace(current) {
                    parsed.push(existing);
                }
            }
            _ => {
                parsed.push(current);
            }
        }
    }

    if let Some(existing) = pending_short {
        parsed.push(existing);
    }

    parsed
}

fn looks_like_option_block(option_text: &str, cmd_name: &str) -> bool {
    let Some(first_line) = option_text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with(".PD"))
    else {
        return false;
    };

    !parse_option_declaration(first_line, cmd_name).is_empty()
}

fn merge_arg(existing: &mut Arg, incoming: ParsedOption, description: &str) {
    if existing.short.is_none() {
        existing.short = incoming.short;
    }
    if existing.long.is_none() {
        existing.long = incoming.long;
    }
    if existing.value_name.is_none() {
        existing.value_name = incoming.value_name;
    }
    if existing.num_args.is_none() {
        existing.num_args = incoming.num_args;
    }
    if existing
        .description
        .as_deref()
        .unwrap_or_default()
        .is_empty()
        && !description.is_empty()
    {
        existing.description = Some(description.to_string());
    }
}

fn add_option(cmd: &mut Command, option_text: &str, description: &str) -> bool {
    let description = clean_sentence(&normalize_text(
        description,
        cmd.name.as_deref().unwrap_or(""),
    ));
    let parsed_options = parse_option_declaration(option_text, cmd.name.as_deref().unwrap_or(""));
    let mut added = false;

    for parsed in parsed_options {
        let key_short = parsed.short.clone();
        let key_long = parsed.long.clone();
        if key_short.is_none() && key_long.is_none() {
            continue;
        }

        if let Some(existing) = cmd.args.iter_mut().find(|arg| {
            (key_short.is_some() && arg.short == key_short)
                || (key_long.is_some() && arg.long == key_long)
        }) {
            merge_arg(existing, parsed, &description);
        } else {
            cmd.args.push(Arg {
                short: parsed.short,
                long: parsed.long,
                description: if description.is_empty() {
                    None
                } else {
                    Some(description.clone())
                },
                value_name: parsed.value_name,
                num_args: parsed.num_args,
                ..Default::default()
            });
            added = true;
        }
    }

    added
}

fn section_title(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let title = trimmed
        .strip_prefix(".SH ")
        .or_else(|| trimmed.strip_prefix(".Sh "))?
        .trim()
        .trim_matches('"');
    Some(title.to_string())
}

fn extract_section<'a>(content: &'a str, names: &[&str]) -> Option<&'a str> {
    let mut start = None;
    let mut offset = 0;

    for line in content.split_inclusive('\n') {
        let line_start = offset;
        offset += line.len();

        if let Some(title) = section_title(line) {
            if start.is_none() && names.iter().any(|name| *name == title) {
                start = Some(offset);
                continue;
            }

            if let Some(section_start) = start {
                return Some(&content[section_start..line_start]);
            }
        }
    }

    start.map(|section_start| &content[section_start..])
}

fn top_level_sections(content: &str) -> Vec<&str> {
    let mut sections = Vec::new();
    let mut current_start = None;
    let mut offset = 0;

    for line in content.split_inclusive('\n') {
        let line_start = offset;
        offset += line.len();

        if section_title(line).is_some() {
            if let Some(section_start) = current_start.replace(line_start) {
                sections.push(&content[section_start..line_start]);
            }
        }
    }

    if let Some(section_start) = current_start {
        sections.push(&content[section_start..]);
    }

    sections
}

fn parse_type1_blocks(cmd: &mut Command, section: &str) -> bool {
    let mut found = false;
    let cmd_name = cmd.name.clone().unwrap_or_default();

    let lines: Vec<&str> = section.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim();
        if line.starts_with(".PP")
            || line.starts_with(".sp")
            || line.starts_with(".SS")
            || line.starts_with(".Ss")
        {
            let mut opt_lines = Vec::new();
            i += 1;

            let mut rs_found = false;
            while i < lines.len() {
                let next_line = lines[i].trim();
                if next_line.starts_with(".RS") {
                    rs_found = true;
                    break;
                }
                if next_line.starts_with(".PP")
                    || next_line.starts_with(".sp")
                    || next_line.starts_with(".SS")
                    || next_line.starts_with(".Ss")
                {
                    break;
                }
                opt_lines.push(lines[i]);
                i += 1;
            }

            if rs_found {
                i += 1; // skip .RS
                let mut desc_lines = Vec::new();
                let mut nesting = 1;

                while i < lines.len() && nesting > 0 {
                    let next_line = lines[i].trim();
                    if next_line.starts_with(".RS") {
                        nesting += 1;
                    } else if next_line.starts_with(".RE") {
                        nesting -= 1;
                    }
                    if nesting > 0 {
                        desc_lines.push(lines[i]);
                        i += 1;
                    }
                }

                if nesting == 0 {
                    let option_text = opt_lines.join("\n");
                    let description = desc_lines.join("\n");

                    if looks_like_option_block(&option_text, &cmd_name) {
                        found |= add_option(cmd, &option_text, &description);
                    }
                    i += 1; // skip .RE
                    continue;
                }
            }
        } else {
            i += 1;
        }
    }

    found
}

fn split_option_and_desc(line: &str) -> (String, Option<String>) {
    let trimmed = line.trim();
    if !trimmed.starts_with("\\f") {
        return (line.to_string(), None);
    }

    let mut in_format = false;
    let mut chars = trimmed.char_indices().peekable();
    let mut split_idx = None;
    let mut saw_format = false;

    while let Some(&(i, c)) = chars.peek() {
        if trimmed[i..].starts_with("\\f") {
            saw_format = true;
            let rest = &trimmed[i + 2..];
            if rest.starts_with('R')
                || rest.starts_with('P')
                || rest.starts_with("(R")
                || rest.starts_with("[R]")
                || rest.starts_with("(P")
                || rest.starts_with("[P]")
            {
                in_format = false;
            } else {
                in_format = true;
            }
            chars.next();
            chars.next();
            if let Some(&(_, next_c)) = chars.peek() {
                if next_c == '(' {
                    chars.next();
                    chars.next();
                    chars.next();
                } else if next_c == '[' {
                    while let Some(&(_, inside_c)) = chars.peek() {
                        chars.next();
                        if inside_c == ']' {
                            break;
                        }
                    }
                } else {
                    chars.next();
                }
            }
            continue;
        }

        if !in_format && saw_format && c.is_alphanumeric() {
            split_idx = Some(i);
            break;
        }

        chars.next();
    }

    if let Some(idx) = split_idx {
        let opt = trimmed[..idx].trim().to_string();
        let desc = trimmed[idx..].trim().to_string();
        let opt_cleaned = if opt.ends_with(':') {
            opt[..opt.len() - 1].trim().to_string()
        } else {
            opt
        };
        (opt_cleaned, Some(desc))
    } else {
        (line.to_string(), None)
    }
}

fn parse_tagged_blocks(cmd: &mut Command, section: &str) -> bool {
    let mut found = false;
    let no_ix = Regex::new(r"(?m)^\.IX.*\n?")
        .unwrap()
        .replace_all(section, "")
        .into_owned();

    let trailing_digits = Regex::new(r"\d+$").unwrap();
    let structural_macro =
        Regex::new(r"^\.(?:TP|TQ|IP|SH|Sh|SS|Ss|UNINDENT|UN|PP|Pp|RS|RE|sp)\b").unwrap();
    let conditional_structural_macro =
        Regex::new(r"^\.(?:ie|el)\b.*\.(?:TP|TQ|IP|HP|SH|Sh|SS|Ss|UNINDENT|UN|PP|Pp|RS|RE|sp)\b")
            .unwrap();
    let pd_macro = Regex::new(r"^\.PD(?:\s+\d+)?$").unwrap();
    let mut lines = no_ix.lines().peekable();

    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        let is_tp = trimmed.starts_with(".TP") || trimmed.starts_with(".TQ");
        let is_hp = trimmed.starts_with(".HP");
        let is_ip = trimmed.starts_with(".IP ");
        if !is_tp && !is_ip && !is_hp {
            continue;
        }

        let mut opt_desc_first_line: Option<String> = None;
        let mut option_from_next_line = false;

        let mut option_name = if is_ip {
            let ip_val = trailing_digits
                .replace(trimmed.trim_start_matches(".IP").trim(), "")
                .into_owned();
            let cleaned_ip = clean_option_source(&ip_val, cmd.name.as_deref().unwrap_or(""));
            if cleaned_ip.starts_with('-') {
                ip_val
            } else {
                option_from_next_line = true;
                let mut option_line = String::new();
                while let Some(next) = lines.peek() {
                    let next_trimmed = next.trim();
                    if next_trimmed.is_empty() {
                        lines.next();
                        continue;
                    }
                    option_line = (*next).to_string();
                    lines.next();
                    break;
                }
                while let Some(next) = lines.peek() {
                    let next_trimmed = next.trim();
                    if next_trimmed.is_empty()
                        || !next_trimmed.starts_with('.')
                        || structural_macro.is_match(next_trimmed)
                    {
                        break;
                    }
                    option_line.push(' ');
                    option_line.push_str(next_trimmed);
                    lines.next();
                }
                let (opt, desc) = split_option_and_desc(&option_line);
                if let Some(d) = desc {
                    opt_desc_first_line = Some(d);
                }
                opt
            }
        } else {
            let mut option_line = String::new();
            while let Some(next) = lines.peek() {
                let next_trimmed = next.trim();
                if next_trimmed.is_empty() {
                    lines.next();
                    continue;
                }
                option_line = (*next).to_string();
                lines.next();
                break;
            }
            while let Some(next) = lines.peek() {
                let next_trimmed = next.trim();
                if next_trimmed.is_empty()
                    || !next_trimmed.starts_with('.')
                    || structural_macro.is_match(next_trimmed)
                {
                    break;
                }
                option_line.push(' ');
                option_line.push_str(next_trimmed);
                lines.next();
            }
            option_line
        };

        if is_ip && !option_from_next_line {
            while let Some(next) = lines.peek() {
                let next_trimmed = next.trim();
                if next_trimmed.is_empty() || pd_macro.is_match(next_trimmed) {
                    lines.next();
                    continue;
                }
                if next_trimmed.starts_with(".IP ") {
                    option_name.push_str(", ");
                    option_name.push_str(
                        &trailing_digits
                            .replace(next_trimmed.trim_start_matches(".IP").trim(), "")
                            .into_owned(),
                    );
                    lines.next();
                    continue;
                }
                break;
            }
        }

        let mut desc_lines = Vec::new();
        if let Some(first) = opt_desc_first_line {
            desc_lines.push(first);
        }
        while let Some(next) = lines.peek() {
            let next_trimmed = next.trim();
            if next_trimmed.is_empty() || pd_macro.is_match(next_trimmed) {
                lines.next();
                continue;
            }
            if is_hp && (next_trimmed == ".IP" || next_trimmed.starts_with(".IP ")) {
                lines.next();
                continue;
            }
            if conditional_structural_macro.is_match(next_trimmed) {
                break;
            }
            if next_trimmed.starts_with(".TP")
                || next_trimmed.starts_with(".TQ")
                || next_trimmed.starts_with(".IP ")
                || next_trimmed.starts_with(".HP")
                || next_trimmed.starts_with(".SH")
                || next_trimmed.starts_with(".Sh")
                || next_trimmed.starts_with(".SS")
                || next_trimmed.starts_with(".Ss")
                || next_trimmed.starts_with(".UNINDENT")
                || next_trimmed == ".UN"
            {
                break;
            }
            desc_lines.push((*next).to_string());
            lines.next();
        }

        found |= add_option(cmd, &option_name, &desc_lines.join("\n"));
    }

    found
}

fn parse_scdoc_blocks(cmd: &mut Command, section: &str) -> bool {
    let mut found = false;
    let re = Regex::new(r"(?ms)(.*?)\.RE").unwrap();
    let mut cursor = section;

    while let Some(caps) = re.captures(cursor) {
        let block = caps.get(1).unwrap().as_str();
        let cleaned: Vec<String> = block
            .lines()
            .map(|line| normalize_whitespace(&strip_groff_wrappers(line)))
            .filter(|line| !line.is_empty() && line != ".P" && line != ".RS 4")
            .collect();

        if cleaned.len() >= 2 {
            found |= add_option(cmd, &cleaned[0], &cleaned[1]);
        }

        cursor = &cursor[caps.get(0).unwrap().end()..];
    }

    found
}

fn parse_darwin_option_line(line: &str) -> Option<String> {
    if !line.starts_with(".It Fl") {
        return None;
    }

    let mut text = line.trim().trim_start_matches(".It").trim().to_string();
    text = format!(" {text} ")
        .replace(" Ns = Ns ", "=")
        .replace(" Ns ", "")
        .replace(" Oo ", "[")
        .replace(" Oc ", "] ")
        .replace(" Op ", "[")
        .replace(" Fl Fl ", " --")
        .replace(" Fl ", " -");
    text = Regex::new(r"(?P<prefix>^|[\s=\[:])Ar\s+")
        .unwrap()
        .replace_all(&text, "${prefix}")
        .into_owned();
    let declaration = normalize_whitespace(&strip_groff_wrappers(&text));
    if !declaration.contains('-') {
        return None;
    }

    Some(declaration)
}

fn parse_darwin(cmd: &mut Command, section: &str) -> bool {
    let mut found = false;
    let mut lines = section.lines().peekable();

    while let Some(line) = lines.next() {
        let Some(option_name) = parse_darwin_option_line(line) else {
            continue;
        };

        let mut desc_lines = Vec::new();
        while let Some(next) = lines.peek() {
            if next.starts_with(".It Fl") || next.starts_with(".Sh") || next.starts_with(".SH") {
                break;
            }
            if let Some(next_line) = strip_line_comment(next) {
                let text = normalize_text(&next_line, cmd.name.as_deref().unwrap_or(""));
                if !text.is_empty() {
                    desc_lines.push(text);
                }
            }
            lines.next();
        }

        found |= add_option(cmd, &option_name, &desc_lines.join(" "));
    }

    found
}

fn deroff(content: &str, cmd_name: &str) -> String {
    let mut out = Vec::new();

    for raw_line in content.lines() {
        let Some(raw_line) = strip_line_comment(raw_line) else {
            continue;
        };
        let trimmed = raw_line.trim_start();

        if trimmed.starts_with(".Sh ") || trimmed.starts_with(".SH ") {
            out.push(trimmed[4..].trim().trim_matches('"').to_uppercase());
            continue;
        }
        if trimmed.starts_with(".Ss ") || trimmed.starts_with(".SS ") {
            out.push(trimmed[4..].trim().trim_matches('"').to_uppercase());
            continue;
        }
        if let Some(option_name) = parse_darwin_option_line(trimmed) {
            out.push(option_name);
            continue;
        }
        if trimmed.starts_with(".PP")
            || trimmed.starts_with(".Pp")
            || trimmed.starts_with(".IP")
            || trimmed.starts_with(".TP")
            || trimmed.starts_with(".TQ")
            || trimmed.starts_with(".RS")
            || trimmed.starts_with(".RE")
            || trimmed.starts_with(".Bl")
            || trimmed.starts_with(".El")
        {
            out.push(String::new());
            continue;
        }

        let line = normalize_text(&raw_line, cmd_name);
        if !line.is_empty() {
            out.push(line);
        }
    }

    out.join("\n")
}

fn parse_deroff(cmd: &mut Command, content: &str) -> bool {
    let text = deroff(content, cmd.name.as_deref().unwrap_or(""));
    let mut lines: Vec<&str> = text.lines().collect();

    while let Some(line) = lines.first() {
        let upper = line.trim().to_uppercase();
        if upper == "DESCRIPTION" || upper == "OPTIONS" || upper == "COMMAND OPTIONS" {
            break;
        }
        lines.remove(0);
    }

    let mut found = false;
    let mut index = 0;
    while index < lines.len() {
        let line = normalize_whitespace(lines[index]);
        if line.is_empty() {
            index += 1;
            continue;
        }

        let upper = line.to_uppercase();
        if upper == "BUGS" || upper == "EXAMPLES" || upper == "FILES" {
            break;
        }
        if !line.starts_with('-') {
            index += 1;
            continue;
        }

        let option_line = line;
        index += 1;
        let mut desc_parts = Vec::new();

        while index < lines.len() {
            let next = normalize_whitespace(lines[index]);
            let upper = next.to_uppercase();
            if next.is_empty() {
                index += 1;
                if !desc_parts.is_empty() {
                    break;
                }
                continue;
            }
            if next.starts_with('-') || upper == "BUGS" || upper == "EXAMPLES" || upper == "FILES" {
                break;
            }
            desc_parts.push(next);
            index += 1;
        }

        found |= add_option(cmd, &option_line, &desc_parts.join(" "));
    }

    found
}

fn parse_type1(cmd: &mut Command, content: &str) -> bool {
    let mut found = false;
    if let Some(section) = extract_section(content, &["OPTIONS"]) {
        found |= parse_type1_blocks(cmd, section);
        if !found {
            found |= parse_tagged_blocks(cmd, section);
        }
    }
    found
}

fn parse_type2(cmd: &mut Command, content: &str) -> bool {
    extract_section(content, &["OPTIONS"])
        .map(|section| parse_tagged_blocks(cmd, section))
        .unwrap_or(false)
}

fn parse_type3(cmd: &mut Command, content: &str) -> bool {
    extract_section(content, &["DESCRIPTION"])
        .map(|section| parse_tagged_blocks(cmd, section))
        .unwrap_or(false)
}

fn parse_type4(cmd: &mut Command, content: &str) -> bool {
    extract_section(content, &["FUNCTION LETTERS"])
        .map(|section| parse_tagged_blocks(cmd, section))
        .unwrap_or(false)
}

fn parse_scdoc(cmd: &mut Command, content: &str) -> bool {
    if !content.contains("Generated by scdoc") {
        return false;
    }
    extract_section(content, &["OPTIONS"])
        .map(|section| parse_scdoc_blocks(cmd, section))
        .unwrap_or(false)
}

fn parse_darwin_sections(cmd: &mut Command, content: &str) -> bool {
    let mut found = false;
    if let Some(section) = extract_section(content, &["DESCRIPTION"]) {
        found |= parse_darwin(cmd, section);
    }
    if !found {
        if let Some(section) = extract_section(content, &["OPTIONS"]) {
            found |= parse_darwin(cmd, section);
        }
    }
    found
}

fn parse_subcommand_name(cmd_name: &str, token: &str) -> Option<String> {
    let normalized = normalize_whitespace(&strip_groff_wrappers(token));
    let caps = Regex::new(r"^(?P<name>[A-Za-z0-9][A-Za-z0-9+._-]*)\((?P<section>\d+)\)$")
        .unwrap()
        .captures(&normalized)?;
    let name = caps.name("name").unwrap().as_str();
    let prefix = format!("{cmd_name}-");
    let stripped = name.strip_prefix(&prefix)?;

    if stripped.is_empty() {
        return None;
    }

    Some(stripped.to_string())
}

fn add_subcommand(cmd: &mut Command, name: &str, description: &str) -> bool {
    let description = clean_sentence(&normalize_text(
        description,
        cmd.name.as_deref().unwrap_or(""),
    ));

    if let Some(existing) = cmd
        .subcommands
        .iter_mut()
        .find(|subcommand| subcommand.name.as_deref() == Some(name))
    {
        if existing
            .description
            .as_deref()
            .unwrap_or_default()
            .is_empty()
            && !description.is_empty()
        {
            existing.description = Some(description);
        }
        return false;
    }

    cmd.subcommands.push(Command {
        name: Some(name.to_string()),
        aliases: Vec::new(),
        description: if description.is_empty() {
            None
        } else {
            Some(description)
        },
        args: Vec::new(),
        subcommands: Vec::new(),
        author: None,
    });
    true
}

fn extract_subcommand_candidates(section: &str, cmd_name: &str) -> Vec<(String, String)> {
    let re = Regex::new(r"(?ms)\.PP\s*(.*?)\.RS 4\s*(.*?)\.RE").unwrap();
    let mut candidates = Vec::new();

    for caps in re.captures_iter(section) {
        let raw_name = caps.get(1).unwrap().as_str();
        let raw_description = caps.get(2).unwrap().as_str();
        let Some(name) = parse_subcommand_name(cmd_name, raw_name) else {
            continue;
        };
        candidates.push((name, raw_description.to_string()));
    }

    if candidates.is_empty() {
        let lines: Vec<&str> = section.lines().collect();
        let mut i = 0;
        while i < lines.len() {
            let line = lines[i].trim();
            if let Some(name) = parse_subcommand_name(cmd_name, line) {
                i += 1;
                if i < lines.len() && lines[i].trim() == ".br" {
                    i += 1;
                }

                let mut desc_parts = Vec::new();
                while i < lines.len() {
                    let next_line = lines[i].trim();
                    if next_line.starts_with(".sp")
                        || next_line.starts_with(".SS")
                        || next_line.starts_with(".SH")
                        || parse_subcommand_name(cmd_name, next_line).is_some()
                    {
                        break;
                    }
                    if !next_line.is_empty() {
                        desc_parts.push(next_line);
                    }
                    i += 1;
                }
                candidates.push((name, desc_parts.join("\n")));
            } else {
                i += 1;
            }
        }
    }

    candidates
}

fn parse_subcommands(cmd: &mut Command, content: &str) -> bool {
    let Some(cmd_name) = cmd.name.clone() else {
        return false;
    };

    let mut found = false;

    for section in top_level_sections(content) {
        let candidates = extract_subcommand_candidates(section, &cmd_name);
        let is_commands_section = section
            .lines()
            .next()
            .map(|l| {
                let upper = l.to_uppercase();
                upper.contains("COMMAND") || upper.contains("SUBCOMMAND")
            })
            .unwrap_or(false);

        if !is_commands_section && candidates.len() < 3 {
            continue;
        }

        for (name, description) in candidates {
            found |= add_subcommand(cmd, &name, &description);
        }
    }

    found
}

fn parse_manpage_base(cmd_name: &str, content: &str) -> Option<Command> {
    let mut cmd = Command {
        name: Some(cmd_name.to_string()),
        aliases: Vec::new(),
        description: None,
        args: Vec::new(),
        subcommands: Vec::new(),
        author: None,
    };

    parse_subcommands(&mut cmd, content);

    let parsers: [fn(&mut Command, &str) -> bool; 7] = [
        parse_scdoc,
        parse_type1,
        parse_type2,
        parse_type4,
        parse_type3,
        parse_darwin_sections,
        parse_deroff,
    ];

    for parser in parsers {
        let before = cmd.args.len();
        let success = parser(&mut cmd, content);
        if success && cmd.args.len() > before {
            break;
        }
    }

    if cmd.args.is_empty() && cmd.subcommands.is_empty() {
        None
    } else {
        // Expand bracketed negation flags (like --[no-]color) into both variants
        cmd.expand_no_options();
        cmd.populate_possible_values();
        Some(cmd)
    }
}

pub fn parse_manpage(cmd_name: &str, content: &str) -> Option<Command> {
    parse_manpage_base(cmd_name, content)
}

pub fn parse_manpage_recursive<F>(
    cmd_name: &str,
    content: &str,
    max_depth: usize,
    loader: &F,
) -> Option<Command>
where
    F: Fn(&str) -> Option<String>,
{
    let mut cmd = parse_manpage_base(cmd_name, content)?;
    let mut cmd_path = vec![cmd_name.to_string()];
    parse_manpage_recursive_impl(&mut cmd, &mut cmd_path, max_depth, loader);
    Some(cmd)
}

fn parse_manpage_recursive_impl<F>(
    cmd: &mut Command,
    cmd_path: &mut Vec<String>,
    max_depth: usize,
    loader: &F,
) where
    F: Fn(&str) -> Option<String>,
{
    if cmd_path.len() - 1 >= max_depth {
        return;
    }

    let num_subcommands = cmd.subcommands.len();
    for idx in 0..num_subcommands {
        let sub_name = match &cmd.subcommands[idx].name {
            Some(n) => n.clone(),
            None => continue,
        };

        cmd_path.push(sub_name.clone());
        let sub_man_name = cmd_path.join("-");

        if let Some(sub_content) = loader(&sub_man_name) {
            if let Some(parsed_sub) = parse_manpage_base(&sub_man_name, &sub_content) {
                let target = &mut cmd.subcommands[idx];
                target.args = parsed_sub.args;
                if target.description.is_none()
                    || target.description.as_deref().unwrap_or("").is_empty()
                {
                    target.description = parsed_sub.description;
                }
                target.subcommands = parsed_sub.subcommands;

                parse_manpage_recursive_impl(target, cmd_path, max_depth, loader);
            }
        }
        cmd_path.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const TYPE1_FIXTURE: &str = r#".TH EXAMPLE 1
.SH "OPTIONS"
.PP
.BI \-a \&,
.BI \-\^-all
.RS 4
Show hidden files and include dot entries.
.RE
.PP
.BI \-o \ output \&,
.BI \-\^-output = file
.RS 4
Write the generated report to the chosen file path.
.RE
.PP
.BI \-d [debug-file] \&,
.BI \-\^-debug [=debug-file]
.RS 4
Enable debug logging and optionally write traces to debug-file.
.RE
"#;

    const TYPE2_FIXTURE: &str = r#".TH SAMPLE 1
.SH OPTIONS
.TP
.B \-n
Number output lines before printing them.
.TP
.BI \-f \ input-file
Read input from input-file instead of stdin.
.TP
.BI \-\^-format = json
Render the output using the requested json format.
"#;

    const DARWIN_FIXTURE: &str = r#".Dd January 1 2026
.Dt SAMPLE 1
.Os
.Sh DESCRIPTION
The options are as follows:
.Bl -tag -width Ds -compact
.It Fl a
Enable agent forwarding for the current connection.
.It Fl b Ar bind_address
Bind to bind_address before opening the remote session.
.It Fl Fl verbose
Produce verbose logs for each connection phase.
.El
"#;

    const DEROFF_FIXTURE: &str = r#".TH RAW 1
.SH DESCRIPTION
-q, --quiet
Suppress normal output while still reporting errors.

-p PATH, --path PATH
Read files from PATH before applying filters.

BUGS
None documented.
"#;

    use crate::test_helpers::*;

    fn parse_test_manpage(name: &str) -> Command {
        let content = fs::read_to_string(format!("../tests/man_pages/{name}")).unwrap();
        let cmd_name = name.split('.').next().unwrap();
        parse_manpage(cmd_name, &content).unwrap()
    }

    fn parse_test_manpage_recursive(name: &str, max_depth: usize) -> Command {
        let content = fs::read_to_string(format!("../tests/man_pages/{name}")).unwrap();
        let cmd_name = name.split('.').next().unwrap();
        let loader = |sub_man_name: &str| -> Option<String> {
            if let Ok(c) = fs::read_to_string(format!("../tests/man_pages/{sub_man_name}.1")) {
                return Some(c);
            }
            if let Ok(c) = fs::read_to_string(format!("../tests/man_pages/{sub_man_name}.8")) {
                return Some(c);
            }
            None
        };
        parse_manpage_recursive(cmd_name, &content, max_depth, &loader).unwrap()
    }

    #[test]
    fn parses_type1_options_exhaustively() {
        let cmd = parse_manpage("example", TYPE1_FIXTURE).unwrap();
        assert_expected_args(
            &cmd,
            &[
                ExpectedArg {
                    arg: Arg {
                        short: Some("-a".to_string()),
                        long: Some("--all".to_string()),
                        value_name: None,
                        num_args: None,
                        ..Default::default()
                    },
                    description_contains: "Show hidden files",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-o".to_string()),
                        long: Some("--output".to_string()),
                        value_name: Some("file".to_string()),
                        num_args: Some("1".to_string()),
                        value_hint: crate::ValueHint::FilePath,
                        ..Default::default()
                    },
                    description_contains: "chosen file path",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-d".to_string()),
                        long: Some("--debug".to_string()),
                        value_name: Some("debug-file".to_string()),
                        num_args: Some("?".to_string()),
                        value_hint: crate::ValueHint::FilePath,
                        ..Default::default()
                    },
                    description_contains: "optionally write traces",
                },
            ],
        );
    }

    #[test]
    fn parses_type2_options_exhaustively() {
        let cmd = parse_manpage("sample", TYPE2_FIXTURE).unwrap();
        assert_expected_args(
            &cmd,
            &[
                ExpectedArg {
                    arg: Arg {
                        short: Some("-n".to_string()),
                        long: None,
                        value_name: None,
                        num_args: None,
                        ..Default::default()
                    },
                    description_contains: "Number output lines",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-f".to_string()),
                        long: None,
                        value_name: Some("input-file".to_string()),
                        num_args: Some("1".to_string()),
                        value_hint: crate::ValueHint::FilePath,
                        ..Default::default()
                    },
                    description_contains: "instead of stdin",
                },
                ExpectedArg {
                    arg: Arg {
                        short: None,
                        long: Some("--format".to_string()),
                        value_name: Some("json".to_string()),
                        num_args: Some("1".to_string()),
                        ..Default::default()
                    },
                    description_contains: "requested json format",
                },
            ],
        );
    }

    #[test]
    fn parses_darwin_options_exhaustively() {
        let cmd = parse_manpage("sample", DARWIN_FIXTURE).unwrap();
        assert_expected_args(
            &cmd,
            &[
                ExpectedArg {
                    arg: Arg {
                        short: Some("-a".to_string()),
                        long: None,
                        value_name: None,
                        num_args: None,
                        ..Default::default()
                    },
                    description_contains: "agent forwarding",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-b".to_string()),
                        long: None,
                        value_name: Some("bind_address".to_string()),
                        num_args: Some("1".to_string()),
                        value_hint: crate::ValueHint::Hostname,
                        ..Default::default()
                    },
                    description_contains: "before opening the remote session",
                },
                ExpectedArg {
                    arg: Arg {
                        short: None,
                        long: Some("--verbose".to_string()),
                        value_name: None,
                        num_args: None,
                        ..Default::default()
                    },
                    description_contains: "verbose logs",
                },
            ],
        );
    }

    #[test]
    fn parses_deroff_options_exhaustively() {
        let cmd = parse_manpage("raw", DEROFF_FIXTURE).unwrap();
        assert_expected_args(
            &cmd,
            &[
                ExpectedArg {
                    arg: Arg {
                        short: Some("-q".to_string()),
                        long: Some("--quiet".to_string()),
                        value_name: None,
                        num_args: None,
                        ..Default::default()
                    },
                    description_contains: "Suppress normal output",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-p".to_string()),
                        long: Some("--path".to_string()),
                        value_name: Some("PATH".to_string()),
                        num_args: Some("1".to_string()),
                        value_hint: crate::ValueHint::AnyPath,
                        ..Default::default()
                    },
                    description_contains: "Read files from PATH",
                },
            ],
        );
    }

    #[test]
    fn parses_real_git_options_with_values_and_descriptions() {
        let cmd = parse_test_manpage("git.1");
        assert!(cmd.subcommands.len() >= 9);
        assert_contains_subcommands(
            &cmd,
            &[
                ("add", "file contents to the index"),
                ("commit", "Record changes to the repository"),
                ("diff", "Show changes between commits"),
                ("fetch", "Download objects and refs"),
                ("init", "Create an empty Git repository"),
                ("log", "Show commit logs"),
                ("pull", "Fetch from and integrate"),
                ("push", "Update remote refs"),
                ("status", "Show the working tree status"),
            ],
        );
        let expected = [
            ExpectedArg {
                arg: Arg {
                    short: Some("-v".to_string()),
                    long: Some("--version".to_string()),
                    value_name: None,
                    num_args: None,
                    ..Default::default()
                },
                description_contains: "Prints the Git suite version",
            },
            ExpectedArg {
                arg: Arg {
                    short: Some("-C".to_string()),
                    long: None,
                    value_name: Some("<path>".to_string()),
                    num_args: Some("1".to_string()),
                    value_hint: crate::ValueHint::DirPath,
                    ..Default::default()
                },
                description_contains: "instead of the current working directory",
            },
            ExpectedArg {
                arg: Arg {
                    short: Some("-c".to_string()),
                    long: None,
                    value_name: Some("<name>=<value>".to_string()),
                    num_args: Some("1".to_string()),
                    ..Default::default()
                },
                description_contains: "override values from configuration files",
            },
            ExpectedArg {
                arg: Arg {
                    short: None,
                    long: Some("--config-env".to_string()),
                    value_name: Some("<name>=<envvar>".to_string()),
                    num_args: Some("1".to_string()),
                    ..Default::default()
                },
                description_contains: "retrieve the value",
            },
        ];

        assert_contains_expected_args(&cmd, &expected);
    }

    #[test]
    fn parses_real_cat_fixture() {
        let cmd = parse_test_manpage("cat.1");
        assert_expected_subcommands(&cmd, &[]);
        assert_contains_expected_args(
            &cmd,
            &[
                ExpectedArg {
                    arg: Arg {
                        short: Some("-A".to_string()),
                        long: Some("--show-all".to_string()),
                        value_name: None,
                        num_args: None,
                        ..Default::default()
                    },
                    description_contains: "equivalent to -vET",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-b".to_string()),
                        long: Some("--number-nonblank".to_string()),
                        value_name: None,
                        num_args: None,
                        ..Default::default()
                    },
                    description_contains: "number nonempty output lines",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-u".to_string()),
                        long: None,
                        value_name: None,
                        num_args: None,
                        ..Default::default()
                    },
                    description_contains: "(ignored)",
                },
                ExpectedArg {
                    arg: Arg {
                        short: None,
                        long: Some("--help".to_string()),
                        value_name: None,
                        num_args: None,
                        ..Default::default()
                    },
                    description_contains: "display this help and exit",
                },
            ],
        );
    }

    #[test]
    fn parses_real_chmod_fixture() {
        let cmd = parse_test_manpage("chmod.1");
        assert_expected_subcommands(&cmd, &[]);
        assert_contains_expected_args(
            &cmd,
            &[
                ExpectedArg {
                    arg: Arg {
                        short: Some("-c".to_string()),
                        long: Some("--changes".to_string()),
                        value_name: None,
                        num_args: None,
                        ..Default::default()
                    },
                    description_contains: "report only when a change is made",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-v".to_string()),
                        long: Some("--verbose".to_string()),
                        value_name: None,
                        num_args: None,
                        ..Default::default()
                    },
                    description_contains: "diagnostic for every file processed",
                },
                ExpectedArg {
                    arg: Arg {
                        short: None,
                        long: Some("--reference".to_string()),
                        value_name: Some("RFILE".to_string()),
                        num_args: Some("1".to_string()),
                        value_hint: crate::ValueHint::FilePath,
                        ..Default::default()
                    },
                    description_contains: "use RFILE's mode",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-R".to_string()),
                        long: Some("--recursive".to_string()),
                        value_name: None,
                        num_args: None,
                        ..Default::default()
                    },
                    description_contains: "change files and directories recursively",
                },
            ],
        );
    }

    #[test]
    fn parses_real_chown_fixture() {
        let cmd = parse_test_manpage("chown.1");
        assert_expected_subcommands(&cmd, &[]);
        assert_contains_expected_args(
            &cmd,
            &[
                ExpectedArg {
                    arg: Arg {
                        short: Some("-c".to_string()),
                        long: Some("--changes".to_string()),
                        value_name: None,
                        num_args: None,
                        ..Default::default()
                    },
                    description_contains: "report only when a change is made",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-h".to_string()),
                        long: Some("--no-dereference".to_string()),
                        value_name: None,
                        num_args: None,
                        ..Default::default()
                    },
                    description_contains: "affect symbolic links instead of any referenced file",
                },
                ExpectedArg {
                    arg: Arg {
                        short: None,
                        long: Some("--from".to_string()),
                        value_name: Some("CURRENT_OWNER:CURRENT_GROUP".to_string()),
                        num_args: Some("1".to_string()),
                        ..Default::default()
                    },
                    description_contains: "only if its current owner and/or group match",
                },
                ExpectedArg {
                    arg: Arg {
                        short: None,
                        long: Some("--reference".to_string()),
                        value_name: Some("RFILE".to_string()),
                        num_args: Some("1".to_string()),
                        value_hint: crate::ValueHint::FilePath,
                        ..Default::default()
                    },
                    description_contains: "use RFILE's owner and group",
                },
            ],
        );
    }

    #[test]
    fn parses_real_cp_fixture() {
        let cmd = parse_test_manpage("cp.1");
        assert_expected_subcommands(&cmd, &[]);
        assert_contains_expected_args(
            &cmd,
            &[
                ExpectedArg {
                    arg: Arg {
                        short: Some("-a".to_string()),
                        long: Some("--archive".to_string()),
                        value_name: None,
                        num_args: None,
                        ..Default::default()
                    },
                    description_contains: "same as -dR --preserve=all",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-b".to_string()),
                        long: None,
                        value_name: None,
                        num_args: None,
                        ..Default::default()
                    },
                    description_contains: "does not accept an argument",
                },
                ExpectedArg {
                    arg: Arg {
                        short: None,
                        long: Some("--attributes-only".to_string()),
                        value_name: None,
                        num_args: None,
                        ..Default::default()
                    },
                    description_contains: "don't copy the file data",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-S".to_string()),
                        long: Some("--suffix".to_string()),
                        value_name: Some("SUFFIX".to_string()),
                        num_args: Some("1".to_string()),
                        ..Default::default()
                    },
                    description_contains: "override the usual backup suffix",
                },
            ],
        );
    }

    #[test]
    fn parses_real_gawk_fixture() {
        let cmd = parse_test_manpage("gawk.1");
        assert_expected_subcommands(&cmd, &[]);
        assert_contains_expected_args(
            &cmd,
            &[
                ExpectedArg {
                    arg: Arg {
                        short: Some("-f".to_string()),
                        long: Some("--file".to_string()),
                        value_name: Some("program-file".to_string()),
                        num_args: Some("1".to_string()),
                        value_hint: crate::ValueHint::FilePath,
                        ..Default::default()
                    },
                    description_contains: "program source from the file",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-F".to_string()),
                        long: Some("--field-separator".to_string()),
                        value_name: Some("fs".to_string()),
                        num_args: Some("1".to_string()),
                        ..Default::default()
                    },
                    description_contains: "input field separator",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-b".to_string()),
                        long: Some("--characters-as-bytes".to_string()),
                        value_name: None,
                        num_args: None,
                        ..Default::default()
                    },
                    description_contains: "single-byte characters",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-c".to_string()),
                        long: Some("--traditional".to_string()),
                        value_name: None,
                        num_args: None,
                        ..Default::default()
                    },
                    description_contains: "compatibility mode",
                },
            ],
        );
    }

    #[test]
    fn parses_real_grep_fixture() {
        let cmd = parse_test_manpage("grep.1");
        assert_expected_subcommands(&cmd, &[]);
        assert_contains_expected_args(
            &cmd,
            &[
                ExpectedArg {
                    arg: Arg {
                        short: Some("-E".to_string()),
                        long: Some("--extended-regexp".to_string()),
                        value_name: None,
                        num_args: None,
                        ..Default::default()
                    },
                    description_contains: "extended regular expressions",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-i".to_string()),
                        long: Some("--ignore-case".to_string()),
                        value_name: None,
                        num_args: None,
                        ..Default::default()
                    },
                    description_contains: "Ignore case distinctions",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-f".to_string()),
                        long: Some("--file".to_string()),
                        value_name: Some("FILE".to_string()),
                        num_args: Some("1".to_string()),
                        value_hint: crate::ValueHint::FilePath,
                        ..Default::default()
                    },
                    description_contains: "Obtain patterns from FILE",
                },
                ExpectedArg {
                    arg: Arg {
                        short: None,
                        long: Some("--no-ignore-case".to_string()),
                        value_name: None,
                        num_args: None,
                        ..Default::default()
                    },
                    description_contains: "Do not ignore case distinctions",
                },
            ],
        );
    }

    #[test]
    fn parses_real_ls_fixture() {
        let cmd = parse_test_manpage("ls.1");
        assert_expected_subcommands(&cmd, &[]);
        assert_contains_expected_args(
            &cmd,
            &[
                ExpectedArg {
                    arg: Arg {
                        short: Some("-a".to_string()),
                        long: Some("--all".to_string()),
                        value_name: None,
                        num_args: None,
                        ..Default::default()
                    },
                    description_contains: "do not ignore entries starting with",
                },
                ExpectedArg {
                    arg: Arg {
                        short: None,
                        long: Some("--author".to_string()),
                        value_name: None,
                        num_args: None,
                        ..Default::default()
                    },
                    description_contains: "print the author of each file",
                },
                ExpectedArg {
                    arg: Arg {
                        short: None,
                        long: Some("--block-size".to_string()),
                        value_name: Some("SIZE".to_string()),
                        num_args: Some("1".to_string()),
                        ..Default::default()
                    },
                    description_contains: "scale sizes by SIZE",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-d".to_string()),
                        long: Some("--directory".to_string()),
                        value_name: None,
                        num_args: None,
                        ..Default::default()
                    },
                    description_contains: "list directories themselves",
                },
            ],
        );
    }

    #[test]
    fn parses_real_mkdir_fixture() {
        let cmd = parse_test_manpage("mkdir.1");
        assert_expected_subcommands(&cmd, &[]);
        assert_contains_expected_args(
            &cmd,
            &[
                ExpectedArg {
                    arg: Arg {
                        short: Some("-m".to_string()),
                        long: Some("--mode".to_string()),
                        value_name: Some("MODE".to_string()),
                        num_args: Some("1".to_string()),
                        ..Default::default()
                    },
                    description_contains: "set file mode",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-p".to_string()),
                        long: Some("--parents".to_string()),
                        value_name: None,
                        num_args: None,
                        ..Default::default()
                    },
                    description_contains: "make parent directories as needed",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-v".to_string()),
                        long: Some("--verbose".to_string()),
                        value_name: None,
                        num_args: None,
                        ..Default::default()
                    },
                    description_contains: "message for each created directory",
                },
                ExpectedArg {
                    arg: Arg {
                        short: None,
                        long: Some("--help".to_string()),
                        value_name: None,
                        num_args: None,
                        ..Default::default()
                    },
                    description_contains: "display this help and exit",
                },
            ],
        );
    }

    #[test]
    fn parses_real_mv_fixture() {
        let cmd = parse_test_manpage("mv.1");
        assert_expected_subcommands(&cmd, &[]);
        assert_contains_expected_args(
            &cmd,
            &[
                ExpectedArg {
                    arg: Arg {
                        short: Some("-b".to_string()),
                        long: None,
                        value_name: None,
                        num_args: None,
                        ..Default::default()
                    },
                    description_contains: "does not accept an argument",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-f".to_string()),
                        long: Some("--force".to_string()),
                        value_name: None,
                        num_args: None,
                        ..Default::default()
                    },
                    description_contains: "do not prompt before overwriting",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-S".to_string()),
                        long: Some("--suffix".to_string()),
                        value_name: Some("SUFFIX".to_string()),
                        num_args: Some("1".to_string()),
                        ..Default::default()
                    },
                    description_contains: "override the usual backup suffix",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-t".to_string()),
                        long: Some("--target-directory".to_string()),
                        value_name: Some("DIRECTORY".to_string()),
                        num_args: Some("1".to_string()),
                        value_hint: crate::ValueHint::DirPath,
                        ..Default::default()
                    },
                    description_contains: "move all SOURCE arguments into DIRECTORY",
                },
            ],
        );
    }

    #[test]
    fn parses_real_ping_fixture() {
        let cmd = parse_test_manpage("ping.8");
        assert_expected_subcommands(&cmd, &[]);
        assert_contains_expected_args(
            &cmd,
            &[
                ExpectedArg {
                    arg: Arg {
                        short: Some("-4".to_string()),
                        long: None,
                        value_name: None,
                        num_args: None,
                        ..Default::default()
                    },
                    description_contains: "Use IPv4 only",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-6".to_string()),
                        long: None,
                        value_name: None,
                        num_args: None,
                        ..Default::default()
                    },
                    description_contains: "Use IPv6 only",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-a".to_string()),
                        long: None,
                        value_name: None,
                        num_args: None,
                        ..Default::default()
                    },
                    description_contains: "Audible ping",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-c".to_string()),
                        long: None,
                        value_name: Some("count".to_string()),
                        num_args: Some("1".to_string()),
                        ..Default::default()
                    },
                    description_contains: "Stop after sending count",
                },
            ],
        );
    }

    #[test]
    fn parses_real_rm_fixture() {
        let cmd = parse_test_manpage("rm.1");
        assert_expected_subcommands(&cmd, &[]);
        assert_contains_expected_args(
            &cmd,
            &[
                ExpectedArg {
                    arg: Arg {
                        short: Some("-f".to_string()),
                        long: Some("--force".to_string()),
                        value_name: None,
                        num_args: None,
                        ..Default::default()
                    },
                    description_contains: "ignore nonexistent files and arguments",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-i".to_string()),
                        long: None,
                        value_name: None,
                        num_args: None,
                        ..Default::default()
                    },
                    description_contains: "prompt before every removal",
                },
                ExpectedArg {
                    arg: Arg {
                        short: None,
                        long: Some("--interactive".to_string()),
                        value_name: Some("WHEN".to_string()),
                        num_args: Some("?".to_string()),
                        ..Default::default()
                    },
                    description_contains: "prompt according to WHEN",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-R".to_string()),
                        long: Some("--recursive".to_string()),
                        value_name: None,
                        num_args: None,
                        ..Default::default()
                    },
                    description_contains: "remove directories and their contents recursively",
                },
            ],
        );
    }

    #[test]
    fn parses_real_sed_fixture() {
        let cmd = parse_test_manpage("sed.1");
        assert_expected_subcommands(&cmd, &[]);
        assert_contains_expected_args(
            &cmd,
            &[
                ExpectedArg {
                    arg: Arg {
                        short: Some("-n".to_string()),
                        long: Some("--quiet".to_string()),
                        value_name: None,
                        num_args: None,
                        ..Default::default()
                    },
                    description_contains: "suppress automatic printing of pattern space",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-e".to_string()),
                        long: Some("--expression".to_string()),
                        value_name: Some("script".to_string()),
                        num_args: Some("1".to_string()),
                        ..Default::default()
                    },
                    description_contains: "add the script to the commands",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-i".to_string()),
                        long: Some("--in-place".to_string()),
                        value_name: Some("SUFFIX".to_string()),
                        num_args: Some("?".to_string()),
                        ..Default::default()
                    },
                    description_contains: "edit files in place",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-u".to_string()),
                        long: Some("--unbuffered".to_string()),
                        value_name: None,
                        num_args: None,
                        ..Default::default()
                    },
                    description_contains: "load minimal amounts of data",
                },
            ],
        );
    }

    #[test]
    fn parses_real_wget_fixture() {
        let cmd = parse_test_manpage("wget.1");
        assert_expected_subcommands(&cmd, &[]);
        assert!(
            cmd.args
                .iter()
                .all(|arg| arg.long.as_deref() != Some("--no-"))
        );
        assert_contains_expected_args(
            &cmd,
            &[
                ExpectedArg {
                    arg: Arg {
                        short: Some("-V".to_string()),
                        long: Some("--version".to_string()),
                        value_name: None,
                        num_args: None,
                        ..Default::default()
                    },
                    description_contains: "Display the version of Wget",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-b".to_string()),
                        long: Some("--background".to_string()),
                        value_name: None,
                        num_args: None,
                        ..Default::default()
                    },
                    description_contains: "Go to background immediately after startup",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-o".to_string()),
                        long: Some("--output-file".to_string()),
                        value_name: Some("logfile".to_string()),
                        num_args: Some("1".to_string()),
                        value_hint: crate::ValueHint::FilePath,
                        ..Default::default()
                    },
                    description_contains: "Log all messages to logfile",
                },
                ExpectedArg {
                    arg: Arg {
                        short: None,
                        long: Some("--report-speed".to_string()),
                        value_name: Some("type".to_string()),
                        num_args: Some("1".to_string()),
                        ..Default::default()
                    },
                    description_contains: "Output bandwidth as type",
                },
                ExpectedArg {
                    arg: Arg {
                        short: None,
                        long: Some("--load-cookies".to_string()),
                        value_name: Some("file".to_string()),
                        num_args: Some("1".to_string()),
                        value_hint: crate::ValueHint::FilePath,
                        ..Default::default()
                    },
                    description_contains: "Load cookies from file before the first HTTP retrieval",
                },
            ],
        );
    }

    #[test]
    fn parses_real_find_options_with_descriptions() {
        let cmd = parse_test_manpage("find.1");
        assert_expected_subcommands(&cmd, &[]);
        for item in [
            ExpectedArg {
                arg: Arg {
                    short: Some("-P".to_string()),
                    long: None,
                    value_name: None,
                    num_args: None,
                    ..Default::default()
                },
                description_contains: "Never follow symbolic links",
            },
            ExpectedArg {
                arg: Arg {
                    short: Some("-L".to_string()),
                    long: None,
                    value_name: None,
                    num_args: None,
                    ..Default::default()
                },
                description_contains: "Follow symbolic links",
            },
            ExpectedArg {
                arg: Arg {
                    short: Some("-H".to_string()),
                    long: None,
                    value_name: None,
                    num_args: None,
                    ..Default::default()
                },
                description_contains: "except while processing the command line arguments",
            },
        ] {
            let arg = find_arg(&cmd, &item);
            assert_eq!(arg.short, item.arg.short);
            assert!(normalize_desc(arg.description.as_deref()).contains(item.description_contains));
        }
    }

    #[test]
    fn parses_real_ssh_options_with_values_and_descriptions() {
        let cmd = parse_test_manpage("ssh.1");
        assert_expected_subcommands(&cmd, &[]);
        for item in [
            ExpectedArg {
                arg: Arg {
                    short: Some("-4".to_string()),
                    long: None,
                    value_name: None,
                    num_args: None,
                    ..Default::default()
                },
                description_contains: "IPv4 addresses only",
            },
            ExpectedArg {
                arg: Arg {
                    short: Some("-B".to_string()),
                    long: None,
                    value_name: Some("bind_interface".to_string()),
                    num_args: Some("1".to_string()),
                    ..Default::default()
                },
                description_contains: "Bind to the address",
            },
            ExpectedArg {
                arg: Arg {
                    short: Some("-b".to_string()),
                    long: None,
                    value_name: Some("bind_address".to_string()),
                    num_args: Some("1".to_string()),
                    value_hint: crate::ValueHint::Hostname,
                    ..Default::default()
                },
                description_contains: "source address",
            },
        ] {
            let arg = find_arg(&cmd, &item);
            assert_eq!(arg.short, item.arg.short);
            assert_eq!(arg.value_name, item.arg.value_name);
            assert_eq!(
                arg.value_hint,
                crate::extract_value_hint(arg.value_name.as_deref(), arg.description.as_deref()),
                "ValueHint mismatch for arg {:?} / {:?}",
                arg.short,
                arg.long
            );
            assert!(normalize_desc(arg.description.as_deref()).contains(item.description_contains));
        }
    }

    #[test]
    fn parses_real_sudo_options_with_short_long_pairs() {
        let cmd = parse_test_manpage("sudo.8");
        assert_expected_subcommands(&cmd, &[]);
        for item in [
            ExpectedArg {
                arg: Arg {
                    short: Some("-A".to_string()),
                    long: Some("--askpass".to_string()),
                    value_name: None,
                    num_args: None,
                    ..Default::default()
                },
                description_contains: "requires a password",
            },
            ExpectedArg {
                arg: Arg {
                    short: Some("-a".to_string()),
                    long: Some("--auth-type".to_string()),
                    value_name: Some("type".to_string()),
                    num_args: Some("1".to_string()),
                    ..Default::default()
                },
                description_contains: "authentication",
            },
        ] {
            let arg = find_arg(&cmd, &item);
            assert_eq!(arg.short, item.arg.short);
            assert_eq!(arg.long, item.arg.long);
            assert_eq!(arg.value_name, item.arg.value_name);
            assert_eq!(
                arg.value_hint,
                crate::extract_value_hint(arg.value_name.as_deref(), arg.description.as_deref()),
                "ValueHint mismatch for arg {:?} / {:?}",
                arg.short,
                arg.long
            );
            assert!(normalize_desc(arg.description.as_deref()).contains(item.description_contains));
        }
    }

    #[test]
    fn parses_real_zstd_fixture() {
        let cmd = parse_test_manpage("zstd.1");
        println!("PARSED ARGS: {:#?}", cmd.args);

        // Assertions on the parsed command structure and options
        let keep_item = ExpectedArg {
            arg: Arg {
                short: Some("-k".to_string()),
                long: Some("--keep".to_string()),
                value_name: None,
                num_args: None,
                ..Default::default()
            },
            description_contains: "keep source file(s)",
        };
        let keep_arg = find_arg(&cmd, &keep_item);
        assert_eq!(keep_arg.short.as_deref(), Some("-k"));
        assert_eq!(keep_arg.long.as_deref(), Some("--keep"));
        assert!(normalize_desc(keep_arg.description.as_deref()).contains("keep source file(s)"));

        let rm_item = ExpectedArg {
            arg: Arg {
                short: None,
                long: Some("--rm".to_string()),
                value_name: None,
                num_args: None,
                ..Default::default()
            },
            description_contains: "remove source file(s)",
        };
        let rm_arg = find_arg(&cmd, &rm_item);
        assert_eq!(rm_arg.long.as_deref(), Some("--rm"));

        let decompress_item = ExpectedArg {
            arg: Arg {
                short: Some("-d".to_string()),
                long: Some("--decompress".to_string()),
                value_name: None,
                num_args: None,
                ..Default::default()
            },
            description_contains: "Decompress",
        };
        let decompress_arg = find_arg(&cmd, &decompress_item);
        assert_eq!(decompress_arg.short.as_deref(), Some("-d"));

        let ultra_item = ExpectedArg {
            arg: Arg {
                short: None,
                long: Some("--ultra".to_string()),
                value_name: None,
                num_args: None,
                ..Default::default()
            },
            description_contains: "unlocks high compression levels",
        };
        let ultra_arg = find_arg(&cmd, &ultra_item);
        assert_eq!(ultra_arg.long.as_deref(), Some("--ultra"));
    }

    #[test]
    fn parses_manpage_with_negated_options() {
        const FIXTURE: &str = r#".TH RAW 1
.SH DESCRIPTION
-p, --[no-]progress
Forcibly show/hide the progress counter.

--[no]asyncio
Use asynchronous IO.
"#;
        let cmd = parse_manpage("raw", FIXTURE).unwrap();
        let args = &cmd.args;

        // --[no-]progress should expand to --progress and --no-progress
        // Note: the negated variant --no-progress should NOT have a short flag.
        let progress_base = args
            .iter()
            .find(|a| a.long.as_deref() == Some("--progress"))
            .unwrap();
        assert_eq!(progress_base.short.as_deref(), Some("-p"));

        let progress_neg = args
            .iter()
            .find(|a| a.long.as_deref() == Some("--no-progress"))
            .unwrap();
        assert_eq!(progress_neg.short, None);

        // --[no]asyncio should expand to --asyncio and --noasyncio
        let asyncio_base = args
            .iter()
            .find(|a| a.long.as_deref() == Some("--asyncio"))
            .unwrap();
        let asyncio_neg = args
            .iter()
            .find(|a| a.long.as_deref() == Some("--noasyncio"))
            .unwrap();
        assert_eq!(asyncio_base.short, None);
        assert_eq!(asyncio_neg.short, None);
    }

    #[test]
    fn parses_real_ip_fixture() {
        let cmd = parse_test_manpage("ip.8");
        assert_expected_subcommands(
            &cmd,
            &[
                ("address", "Protocol address management"),
                ("addrlabel", "Label configuration"),
                ("route", "Routing table management"),
                ("rule", "Routing policy"),
                ("neighbor", "Neighbor cache"),
                ("ntable", "Neighbor table"),
                ("tunnel", "IP tunnel"),
                ("tuntap", "TUN/TAP device"),
                ("maddr", "Multicast address"),
                ("link", "Network device"),
            ],
        );
        assert_contains_expected_args(
            &cmd,
            &[
                ExpectedArg {
                    arg: Arg {
                        short: Some("-V".to_string()),
                        long: Some("--Version".to_string()),
                        value_name: None,
                        num_args: None,
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "Print the version",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-b".to_string()),
                        long: Some("--batch".to_string()),
                        value_name: Some("<FILENAME>".to_string()),
                        num_args: Some("1".to_string()),
                        value_enum: None,
                        value_hint: crate::ValueHint::FilePath,
                        description: None,
                    },
                    description_contains: "Read commands from provided file",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-o".to_string()),
                        long: Some("--oneline".to_string()),
                        value_name: None,
                        num_args: None,
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "output each record on a single line",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-s".to_string()),
                        long: Some("--stats".to_string()),
                        value_name: None,
                        num_args: None,
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "Output more information",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-d".to_string()),
                        long: Some("--details".to_string()),
                        value_name: None,
                        num_args: None,
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "Output more detailed",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-j".to_string()),
                        long: Some("--json".to_string()),
                        value_name: None,
                        num_args: None,
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "Output results in JavaScript",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-p".to_string()),
                        long: Some("--pretty".to_string()),
                        value_name: None,
                        num_args: None,
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "indentation for readability",
                },
            ],
        );
    }

    #[test]
    fn parses_real_docker_fixture() {
        let cmd = parse_test_manpage("docker.1");
        assert_expected_subcommands(
            &cmd,
            &[
                ("run", "Run a command"),
                ("exec", "Run a command in a running"),
                ("ps", "List containers"),
                ("build", "Build an image"),
                ("images", "List images"),
                ("pull", "Pull an image"),
                ("push", "Push an image"),
                ("rm", "Remove one or more"),
                ("rmi", "Remove one or more"),
                ("logs", "Fetch the logs"),
            ],
        );
        assert_contains_expected_args(
            &cmd,
            &[
                ExpectedArg {
                    arg: Arg {
                        short: Some("-D".to_string()),
                        long: Some("--debug".to_string()),
                        value_name: Some("true".to_string()),
                        num_args: Some("1".to_string()),
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "Enable debug mode",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-H".to_string()),
                        long: Some("--host".to_string()),
                        value_name: Some("unix:".to_string()),
                        num_args: Some("1".to_string()),
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "socket",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-l".to_string()),
                        long: Some("--log-level".to_string()),
                        value_name: Some("debug".to_string()),
                        num_args: Some("1".to_string()),
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "logging level",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-v".to_string()),
                        long: Some("--version".to_string()),
                        value_name: Some("true".to_string()),
                        num_args: Some("1".to_string()),
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "Print version",
                },
                ExpectedArg {
                    arg: Arg {
                        short: None,
                        long: Some("--tlsverify".to_string()),
                        value_name: Some("is".to_string()),
                        num_args: Some("1".to_string()),
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "Use TLS and verify",
                },
            ],
        );
    }

    #[test]
    fn parses_real_npm_fixture() {
        let cmd = parse_test_manpage("npm.1");
        assert_expected_subcommands(
            &cmd,
            &[
                ("install", "Install a package"),
                ("run", "Run an arbitrary"),
                ("publish", "Publish a package"),
                ("test", "Test a package"),
                ("config", "Manage the npm"),
                ("uninstall", "Uninstall a package"),
                ("version", "Bump a package"),
                ("search", "Search for packages"),
                ("update", "Update packages"),
                ("view", "View package registry"),
            ],
        );
        assert_contains_expected_args(
            &cmd,
            &[
                ExpectedArg {
                    arg: Arg {
                        short: Some("-g".to_string()),
                        long: Some("--global".to_string()),
                        value_name: None,
                        num_args: None,
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "globally",
                },
                ExpectedArg {
                    arg: Arg {
                        short: None,
                        long: Some("--registry".to_string()),
                        value_name: Some("URL".to_string()),
                        num_args: Some("1".to_string()),
                        value_enum: None,
                        value_hint: crate::ValueHint::Url,
                        description: None,
                    },
                    description_contains: "specified npm registry URL",
                },
                ExpectedArg {
                    arg: Arg {
                        short: None,
                        long: Some("--prefix".to_string()),
                        value_name: Some("PATH".to_string()),
                        num_args: Some("1".to_string()),
                        value_enum: None,
                        value_hint: crate::ValueHint::AnyPath,
                        description: None,
                    },
                    description_contains: "install packages to",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-v".to_string()),
                        long: Some("--version".to_string()),
                        value_name: None,
                        num_args: None,
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "version",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-h".to_string()),
                        long: Some("--help".to_string()),
                        value_name: None,
                        num_args: None,
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "help",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-f".to_string()),
                        long: Some("--force".to_string()),
                        value_name: None,
                        num_args: None,
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "Force",
                },
            ],
        );
    }

    #[test]
    fn parses_real_systemctl_fixture() {
        let cmd = parse_test_manpage("systemctl.1");
        assert_expected_subcommands(
            &cmd,
            &[
                ("start", "Start one or more"),
                ("stop", "Stop one or more"),
                ("status", "Show runtime status"),
                ("list-units", "List units"),
                ("enable", "Enable one or more"),
                ("disable", "Disable one or more"),
                ("daemon-reload", "Reload daemon"),
                ("restart", "Start or restart"),
                ("reload", "Reload one or more"),
                ("is-active", "Check whether units"),
            ],
        );
        assert_contains_expected_args(
            &cmd,
            &[
                ExpectedArg {
                    arg: Arg {
                        short: Some("-h".to_string()),
                        long: Some("--help".to_string()),
                        value_name: None,
                        num_args: None,
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "help text",
                },
                ExpectedArg {
                    arg: Arg {
                        short: None,
                        long: Some("--system".to_string()),
                        value_name: None,
                        num_args: None,
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "service manager",
                },
                ExpectedArg {
                    arg: Arg {
                        short: None,
                        long: Some("--user".to_string()),
                        value_name: None,
                        num_args: None,
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "calling user",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-H".to_string()),
                        long: Some("--host".to_string()),
                        value_name: None,
                        num_args: Some("1".to_string()),
                        value_enum: None,
                        value_hint: crate::ValueHint::Hostname,
                        description: None,
                    },
                    description_contains: "remotely",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-a".to_string()),
                        long: Some("--all".to_string()),
                        value_name: None,
                        num_args: None,
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "listing units",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-l".to_string()),
                        long: Some("--full".to_string()),
                        value_name: None,
                        num_args: None,
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "ellipsize",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-r".to_string()),
                        long: Some("--recursive".to_string()),
                        value_name: None,
                        num_args: None,
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "local containers",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-q".to_string()),
                        long: Some("--quiet".to_string()),
                        value_name: None,
                        num_args: None,
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "Suppress printing",
                },
            ],
        );
    }

    #[test]
    fn parses_real_apt_fixture() {
        let cmd = parse_test_manpage("apt.8");
        assert_expected_subcommands(
            &cmd,
            &[
                ("install", "Install packages"),
                ("remove", "Remove packages"),
                ("update", "Update list"),
                ("upgrade", "Upgrade the system"),
                ("list", "List packages"),
                ("search", "Search in package"),
                ("show", "Show package details"),
                ("autoremove", "Automatically remove"),
                ("clean", "Clean package"),
                ("purge", "Purge packages"),
            ],
        );
        assert_contains_expected_args(
            &cmd,
            &[
                ExpectedArg {
                    arg: Arg {
                        short: Some("-y".to_string()),
                        long: Some("--yes".to_string()),
                        value_name: None,
                        num_args: None,
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "Automatic yes",
                },
                ExpectedArg {
                    arg: Arg {
                        short: None,
                        long: Some("--config-file".to_string()),
                        value_name: Some("FILE".to_string()),
                        num_args: Some("1".to_string()),
                        value_enum: None,
                        value_hint: crate::ValueHint::FilePath,
                        description: None,
                    },
                    description_contains: "configuration file",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-o".to_string()),
                        long: Some("--option".to_string()),
                        value_name: Some("OPTION".to_string()),
                        num_args: Some("1".to_string()),
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "Set a configuration",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-h".to_string()),
                        long: Some("--help".to_string()),
                        value_name: None,
                        num_args: None,
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "help",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-v".to_string()),
                        long: Some("--version".to_string()),
                        value_name: None,
                        num_args: None,
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "version",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-d".to_string()),
                        long: Some("--download-only".to_string()),
                        value_name: None,
                        num_args: None,
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "Download packages",
                },
                ExpectedArg {
                    arg: Arg {
                        short: None,
                        long: Some("--simulate".to_string()),
                        value_name: None,
                        num_args: None,
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "Simulate",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-q".to_string()),
                        long: Some("--quiet".to_string()),
                        value_name: None,
                        num_args: None,
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "Quiet",
                },
            ],
        );
    }

    #[test]
    fn parses_real_pip_fixture() {
        let cmd = parse_test_manpage("pip.1");
        assert_expected_subcommands(
            &cmd,
            &[
                ("install", "Install packages"),
                ("uninstall", "Uninstall packages"),
                ("list", "List installed"),
                ("show", "Show information"),
                ("config", "Manage local"),
                ("download", "Download packages"),
                ("freeze", "Output installed"),
                ("check", "Verify installed"),
                ("search", "Search PyPI"),
                ("cache", "Inspect and manage"),
            ],
        );
        assert_contains_expected_args(
            &cmd,
            &[
                ExpectedArg {
                    arg: Arg {
                        short: Some("-v".to_string()),
                        long: Some("--verbose".to_string()),
                        value_name: None,
                        num_args: None,
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "Give more output",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-r".to_string()),
                        long: Some("--requirement".to_string()),
                        value_name: Some("<file>".to_string()),
                        num_args: Some("1".to_string()),
                        value_enum: None,
                        value_hint: crate::ValueHint::FilePath,
                        description: None,
                    },
                    description_contains: "requirements file",
                },
                ExpectedArg {
                    arg: Arg {
                        short: None,
                        long: Some("--log".to_string()),
                        value_name: Some("<path>".to_string()),
                        num_args: Some("1".to_string()),
                        value_enum: None,
                        value_hint: crate::ValueHint::AnyPath,
                        description: None,
                    },
                    description_contains: "verbose appending log",
                },
            ],
        );
    }

    #[test]
    fn parses_real_go_fixture() {
        let cmd = parse_test_manpage("go.1");
        assert_expected_subcommands(
            &cmd,
            &[
                ("build", "Compile packages"),
                ("run", "Compile and run"),
                ("test", "Test Go packages"),
                ("clean", "Remove object files"),
                ("doc", "Show documentation"),
                ("env", "Print Go environment"),
                ("fix", "Update packages"),
                ("fmt", "gofmt"),
                ("generate", "Generate Go files"),
                ("get", "Add dependencies"),
            ],
        );
        assert_contains_expected_args(
            &cmd,
            &[
                ExpectedArg {
                    arg: Arg {
                        short: Some("-x".to_string()),
                        long: None,
                        value_name: None,
                        num_args: None,
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "Print commands",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-o".to_string()),
                        long: None,
                        value_name: Some("OUTPUT".to_string()),
                        num_args: Some("1".to_string()),
                        value_enum: None,
                        value_hint: crate::ValueHint::FilePath,
                        description: None,
                    },
                    description_contains: "Write the resulting file to OUTPUT",
                },
                ExpectedArg {
                    arg: Arg {
                        short: None,
                        long: Some("--work".to_string()),
                        value_name: None,
                        num_args: None,
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "temporary work directory",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-v".to_string()),
                        long: None,
                        value_name: None,
                        num_args: None,
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "Verbose",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-n".to_string()),
                        long: None,
                        value_name: None,
                        num_args: None,
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "Dry run",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-a".to_string()),
                        long: None,
                        value_name: None,
                        num_args: None,
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "Force rebuilding",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-p".to_string()),
                        long: None,
                        value_name: Some("n".to_string()),
                        num_args: Some("1".to_string()),
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "parallel",
                },
                ExpectedArg {
                    arg: Arg {
                        short: None,
                        long: Some("--race".to_string()),
                        value_name: None,
                        num_args: None,
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "data race detection",
                },
            ],
        );
    }

    #[test]
    fn parses_real_gh_fixture() {
        let cmd = parse_test_manpage("gh.1");
        assert_expected_subcommands(
            &cmd,
            &[
                ("auth", "Authenticate gh and git with GitHub"),
                ("browse", "Open the repository in the browser"),
                ("codespace", "Connect to and manage codespaces"),
                ("gist", "Manage gists"),
                ("issue", "Manage issues"),
                ("org", "Manage organizations"),
                ("pr", "Manage pull requests"),
                ("project", "Work with GitHub Projects"),
                ("release", "Manage releases"),
                ("repo", "Manage repositories"),
                ("cache", "Manage Github Actions caches"),
                ("run", "View details about workflow runs"),
                ("workflow", "View details about GitHub Actions workflows"),
                ("alias", "Create command shortcuts"),
                ("api", "Make an authenticated GitHub API request"),
                ("completion", "Generate shell completion scripts"),
                ("config", "Manage configuration for gh"),
                ("extension", "Manage gh extensions"),
                ("gpg-key", "Manage GPG keys"),
                ("label", "Manage labels"),
                ("ruleset", "View info about repo rulesets"),
                (
                    "search",
                    "Search for repositories, issues, and pull requests",
                ),
                ("secret", "Manage GitHub secrets"),
                ("ssh-key", "Manage SSH keys"),
                ("status", "Print information about relevant issues"),
                ("variable", "Manage GitHub Actions variables"),
            ],
        );
        assert_contains_expected_args(
            &cmd,
            &[ExpectedArg {
                arg: Arg {
                    short: None,
                    long: Some("--version".to_string()),
                    value_name: None,
                    num_args: None,
                    value_enum: None,
                    value_hint: crate::ValueHint::Unknown,
                    description: None,
                },
                description_contains: "Show gh version",
            }],
        );
    }

    #[test]
    fn parses_real_curl_fixture() {
        let cmd = parse_test_manpage("curl.1");
        assert_expected_subcommands(&cmd, &[]);
        assert_contains_expected_args(
            &cmd,
            &[
                ExpectedArg {
                    arg: Arg {
                        short: Some("-g".to_string()),
                        long: Some("--globoff".to_string()),
                        value_name: None,
                        num_args: None,
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "URL globbing parser",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-o".to_string()),
                        long: Some("--output".to_string()),
                        value_name: Some("<file>".to_string()),
                        num_args: Some("1".to_string()),
                        value_enum: None,
                        value_hint: crate::ValueHint::FilePath,
                        description: None,
                    },
                    description_contains: "Write output to <file>",
                },
                ExpectedArg {
                    arg: Arg {
                        short: None,
                        long: Some("--abstract-unix-socket".to_string()),
                        value_name: Some("<path>".to_string()),
                        num_args: Some("1".to_string()),
                        value_enum: None,
                        value_hint: crate::ValueHint::AnyPath,
                        description: None,
                    },
                    description_contains: "abstract Unix domain socket",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-s".to_string()),
                        long: Some("--silent".to_string()),
                        value_name: None,
                        num_args: None,
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "Silent or quiet mode",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-A".to_string()),
                        long: Some("--user-agent".to_string()),
                        value_name: Some("<name>".to_string()),
                        num_args: Some("1".to_string()),
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "User-Agent string",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-b".to_string()),
                        long: Some("--cookie".to_string()),
                        value_name: Some("<data".to_string()),
                        num_args: Some("1".to_string()),
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "Pass the data to the HTTP server in the Cookie header",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-d".to_string()),
                        long: Some("--data".to_string()),
                        value_name: Some("<data>".to_string()),
                        num_args: Some("1".to_string()),
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "Sends the specified data",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-H".to_string()),
                        long: Some("--header".to_string()),
                        value_name: Some("<header".to_string()),
                        num_args: Some("1".to_string()),
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "Extra header to include",
                },
            ],
        );
    }

    #[test]
    fn parses_real_tar_fixture() {
        let cmd = parse_test_manpage("tar.1");
        assert_expected_subcommands(&cmd, &[]);
        assert_contains_expected_args(
            &cmd,
            &[
                ExpectedArg {
                    arg: Arg {
                        short: Some("-a".to_string()),
                        long: Some("--auto-compress".to_string()),
                        value_name: None,
                        num_args: None,
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "compression program",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-f".to_string()),
                        long: Some("--file".to_string()),
                        value_name: Some("ARCHIVE".to_string()),
                        num_args: Some("1".to_string()),
                        value_enum: None,
                        value_hint: crate::ValueHint::FilePath,
                        description: None,
                    },
                    description_contains: "archive file or device ARCHIVE",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-v".to_string()),
                        long: Some("--verbose".to_string()),
                        value_name: None,
                        num_args: None,
                        value_enum: None,
                        value_hint: crate::ValueHint::CommandString,
                        description: None,
                    },
                    description_contains: "files processed",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-V".to_string()),
                        long: Some("--label".to_string()),
                        value_name: Some("TEXT".to_string()),
                        num_args: Some("1".to_string()),
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "volume name TEXT",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-w".to_string()),
                        long: Some("--interactive".to_string()),
                        value_name: None,
                        num_args: None,
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "confirmation",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-l".to_string()),
                        long: Some("--check-links".to_string()),
                        value_name: None,
                        num_args: None,
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "links are dumped",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-T".to_string()),
                        long: Some("--files-from".to_string()),
                        value_name: Some("FILE".to_string()),
                        num_args: Some("1".to_string()),
                        value_enum: None,
                        value_hint: crate::ValueHint::FilePath,
                        description: None,
                    },
                    description_contains: "FILE",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-X".to_string()),
                        long: Some("--exclude-from".to_string()),
                        value_name: Some("FILE".to_string()),
                        num_args: Some("1".to_string()),
                        value_enum: None,
                        value_hint: crate::ValueHint::FilePath,
                        description: None,
                    },
                    description_contains: "Exclude",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-P".to_string()),
                        long: Some("--absolute-names".to_string()),
                        value_name: None,
                        num_args: None,
                        value_enum: None,
                        value_hint: crate::ValueHint::FilePath,
                        description: None,
                    },
                    description_contains: "leading slashes",
                },
            ],
        );
    }

    #[test]
    fn parses_real_cargo_fixture() {
        let cmd = parse_test_manpage("cargo.1");
        assert_expected_subcommands(
            &cmd,
            &[
                ("bench", "Execute benchmarks"),
                ("build", "Compile a package"),
                ("check", "Check a local package"),
                ("clean", "Remove artifacts"),
                ("doc", "Build a package"),
                ("fetch", "Fetch dependencies"),
                ("fix", "Automatically fix lint"),
                ("run", "Run a binary"),
                ("rustc", "Compile a package"),
                ("rustdoc", "Build a package"),
                ("test", "Execute unit"),
                ("add", "Add dependencies"),
                ("generate-lockfile", "Generate Cargo"),
                ("info", "Display information"),
                ("locate-project", "Print a JSON"),
                ("metadata", "Output the resolved"),
                ("pkgid", "Print a fully qualified"),
                ("remove", "Remove dependencies"),
                ("tree", "Display a tree"),
                ("update", "Update dependencies"),
                ("vendor", "Vendor all"),
                ("init", "Create a new Cargo"),
                ("install", "Build and install"),
                ("new", "Create a new Cargo"),
                ("search", "Search packages"),
                ("uninstall", "Remove a Rust"),
                ("login", "Save an API"),
                ("logout", "Remove an API"),
                ("owner", "Manage the owners"),
                ("package", "Assemble the local"),
                ("publish", "Upload a package"),
                ("yank", "Remove a pushed"),
                ("report", "Generate and display"),
                ("report-future-incompatibilities", "Reports any crates"),
                ("help", "Display help"),
                ("version", "Show version"),
            ],
        );
        assert_contains_expected_args(
            &cmd,
            &[
                ExpectedArg {
                    arg: Arg {
                        short: Some("-V".to_string()),
                        long: Some("--version".to_string()),
                        value_name: None,
                        num_args: None,
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "Print version info",
                },
                ExpectedArg {
                    arg: Arg {
                        short: None,
                        long: Some("--explain".to_string()),
                        value_name: Some("code".to_string()),
                        num_args: Some("1".to_string()),
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "Run rustc --explain",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-v".to_string()),
                        long: Some("--verbose".to_string()),
                        value_name: None,
                        num_args: None,
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "Use verbose output",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-q".to_string()),
                        long: Some("--quiet".to_string()),
                        value_name: None,
                        num_args: None,
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "Do not print cargo log messages",
                },
                ExpectedArg {
                    arg: Arg {
                        short: None,
                        long: Some("--color".to_string()),
                        value_name: Some("when".to_string()),
                        num_args: Some("1".to_string()),
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "Control when colored output is used",
                },
                ExpectedArg {
                    arg: Arg {
                        short: None,
                        long: Some("--locked".to_string()),
                        value_name: None,
                        num_args: None,
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "Asserts that the exact same dependencies",
                },
                ExpectedArg {
                    arg: Arg {
                        short: None,
                        long: Some("--offline".to_string()),
                        value_name: None,
                        num_args: None,
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "Prevents Cargo from accessing the network",
                },
                ExpectedArg {
                    arg: Arg {
                        short: None,
                        long: Some("--frozen".to_string()),
                        value_name: None,
                        num_args: None,
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "Equivalent to specifying both",
                },
                ExpectedArg {
                    arg: Arg {
                        short: None,
                        long: Some("--config".to_string()),
                        value_name: Some("KEY=VALUE".to_string()),
                        num_args: Some("1".to_string()),
                        value_enum: None,
                        value_hint: crate::ValueHint::AnyPath,
                        description: None,
                    },
                    description_contains: "Overrides a Cargo configuration value",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-C".to_string()),
                        long: None,
                        value_name: Some("PATH".to_string()),
                        num_args: Some("1".to_string()),
                        value_enum: None,
                        value_hint: crate::ValueHint::DirPath,
                        description: None,
                    },
                    description_contains: "Changes the current working directory",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-h".to_string()),
                        long: Some("--help".to_string()),
                        value_name: None,
                        num_args: None,
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "Prints help information",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-Z".to_string()),
                        long: None,
                        value_name: Some("flag".to_string()),
                        num_args: Some("1".to_string()),
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "Unstable (nightly-only) flags",
                },
            ],
        );
    }

    #[test]
    fn parses_manpage_recursive_fixture() {
        // Test Cargo recursion: cargo -> cargo-build, cargo-check, cargo-clean
        let cmd = parse_test_manpage_recursive("cargo.1", 5);

        // Find the "build" subcommand of cargo
        let build_sub = cmd
            .subcommands
            .iter()
            .find(|s| s.name.as_deref() == Some("build"))
            .expect("cargo should have 'build' subcommand");

        // Asserts that the recursively parsed subcommand "build" has options from cargo-build.1
        assert_contains_expected_args(
            build_sub,
            &[
                ExpectedArg {
                    arg: Arg {
                        short: Some("-p".to_string()),
                        long: Some("--package".to_string()),
                        value_name: Some("spec\\[u2026".to_string()),
                        num_args: Some("1".to_string()),
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "Build only the specified packages",
                },
                ExpectedArg {
                    arg: Arg {
                        short: Some("-r".to_string()),
                        long: Some("--release".to_string()),
                        value_name: None,
                        num_args: None,
                        value_enum: None,
                        value_hint: crate::ValueHint::Unknown,
                        description: None,
                    },
                    description_contains: "Build optimized artifacts",
                },
            ],
        );

        // Find the "check" subcommand of cargo
        let check_sub = cmd
            .subcommands
            .iter()
            .find(|s| s.name.as_deref() == Some("check"))
            .expect("cargo should have 'check' subcommand");

        // Asserts check options are populated
        assert!(
            check_sub
                .args
                .iter()
                .any(|a| a.long.as_deref() == Some("--profile"))
        );

        // Test GH recursion: gh -> gh-codespace -> gh-codespace-cp
        let gh_cmd = parse_test_manpage_recursive("gh.1", 5);

        let codespace_sub = gh_cmd
            .subcommands
            .iter()
            .find(|s| s.name.as_deref() == Some("codespace"))
            .expect("gh should have 'codespace' subcommand");

        // Asserts gh-codespace options are populated from gh-codespace.1
        assert_contains_expected_args(
            codespace_sub,
            &[ExpectedArg {
                arg: Arg {
                    short: Some("-c".to_string()),
                    long: Some("--codespace".to_string()),
                    value_name: Some("<name>".to_string()),
                    num_args: Some("1".to_string()),
                    value_enum: None,
                    value_hint: crate::ValueHint::Unknown,
                    description: None,
                },
                description_contains: "Name of the codespace",
            }],
        );

        // gh-codespace should have nested subcommand "cp" from gh-codespace.1
        let cp_sub = codespace_sub
            .subcommands
            .iter()
            .find(|s| s.name.as_deref() == Some("cp"))
            .expect("gh codespace should have 'cp' subcommand");

        // gh codespace cp should have options from gh-codespace-cp.1 (recursive!)
        assert_contains_expected_args(
            cp_sub,
            &[ExpectedArg {
                arg: Arg {
                    short: Some("-r".to_string()),
                    long: Some("--recursive".to_string()),
                    value_name: None,
                    num_args: None,
                    value_enum: None,
                    value_hint: crate::ValueHint::Unknown,
                    description: None,
                },
                description_contains: "Recursively copy directories",
            }],
        );
    }
}
