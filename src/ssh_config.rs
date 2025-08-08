use anyhow::{Context, Result};
use glob::glob;
use home::home_dir;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SshHostEntry {
    pub pattern: String,
    pub hostname: Option<String>,
    pub user: Option<String>,
    pub port: Option<u16>,
    pub other: Vec<(String, String)>,
}

impl SshHostEntry {
    pub fn matches(&self, q: &str) -> bool {
        let mut hay = self.pattern.to_lowercase();
        if let Some(hn) = &self.hostname { hay.push_str(" "); hay.push_str(&hn.to_lowercase()); }
        if let Some(u) = &self.user { hay.push_str(" "); hay.push_str(&u.to_lowercase()); }
        hay.contains(q)
    }
}

pub struct SshConfigFile {
    pub path: PathBuf,
    pub text: String,
}

impl SshConfigFile {
    pub fn load_default() -> Result<Self> {
        let path = default_ssh_config_path();
        Self::load(path)
    }

    pub fn load(path: PathBuf) -> Result<Self> {
        let mut text = String::new();
        if path.exists() {
            std::fs::File::open(&path)?.read_to_string(&mut text)?;
        }
        Ok(Self { path, text })
    }

    pub fn list_hosts(&self) -> Vec<SshHostEntry> {
        parse_hosts_from_text(&self.text)
    }

    pub fn upsert_host(&mut self, entry: &SshHostEntry) -> Result<()> {
        // naive approach: append or replace by pattern - preserves comments by appending
        // Parse existing file to string and rebuild
        let mut text = String::new();
        if self.path.exists() {
            std::fs::File::open(&self.path)?.read_to_string(&mut text)?;
        }

        let mut lines: Vec<&str> = text.lines().collect();
        // Find existing block starting with "Host <pattern>" (exact match)
        let mut start = None;
        for (i, line) in lines.iter().enumerate() {
            if line.trim_start().starts_with("Host ") {
                let rest = line.trim_start()[5..].trim();
                if rest == entry.pattern {
                    start = Some(i);
                    break;
                }
            }
        }

        let new_block = render_host_block(entry);
        let mut new_text = String::new();
        if let Some(i) = start {
            // Replace until next "Host " or EOF
            let mut j = i + 1;
            while j < lines.len() && !lines[j].trim_start().starts_with("Host ") {
                j += 1;
            }
            // Reconstruct
            for l in &lines[..i] {
                new_text.push_str(l);
                new_text.push('\n');
            }
            new_text.push_str(&new_block);
            for l in &lines[j..] {
                new_text.push_str(l);
                new_text.push('\n');
            }
        } else {
            new_text = text;
            if !new_text.ends_with('\n') && !new_text.is_empty() { new_text.push('\n'); }
            new_text.push_str(&new_block);
        }

        // Ensure .ssh dir exists and write
        if let Some(parent) = self.path.parent() { fs::create_dir_all(parent)?; }
        let mut file = OpenOptions::new().create(true).write(true).truncate(true).open(&self.path)?;
        file.write_all(new_text.as_bytes())?;

        // Refresh in-memory
        *self = Self::load(self.path.clone())?;
        Ok(())
    }

    pub fn delete_host(&mut self, pattern: &str) -> Result<()> {
        if !self.path.exists() { return Ok(()); }
        let mut text = String::new();
        std::fs::File::open(&self.path)?.read_to_string(&mut text)?;
        let lines: Vec<&str> = text.lines().collect();

        // Find and remove block with exact pattern
        let mut i = 0;
        let mut new_text = String::new();
        while i < lines.len() {
            if lines[i].trim_start().starts_with("Host ") {
                let rest = lines[i].trim_start()[5..].trim();
                if rest == pattern {
                    // skip this block
                    i += 1;
                    while i < lines.len() && !lines[i].trim_start().starts_with("Host ") { i += 1; }
                    continue;
                }
            }
            new_text.push_str(lines[i]);
            new_text.push('\n');
            i += 1;
        }

        let mut file = OpenOptions::new().create(true).write(true).truncate(true).open(&self.path)?;
        file.write_all(new_text.as_bytes())?;
        *self = Self::load(self.path.clone())?;
        Ok(())
    }
}

fn render_host_block(entry: &SshHostEntry) -> String {
    let mut out = String::new();
    out.push_str(&format!("Host {}\n", entry.pattern));
    if let Some(hn) = &entry.hostname { out.push_str(&format!("    HostName {}\n", hn)); }
    if let Some(u) = &entry.user { out.push_str(&format!("    User {}\n", u)); }
    if let Some(p) = entry.port { out.push_str(&format!("    Port {}\n", p)); }
    for (k, v) in &entry.other { out.push_str(&format!("    {} {}\n", k, v)); }
    out.push('\n');
    out
}

fn default_ssh_config_path() -> PathBuf {
    home_dir()
        .map(|h| h.join(".ssh").join("config"))
        .unwrap_or_else(|| PathBuf::from("~/.ssh/config"))
}

fn parse_hosts_from_text(text: &str) -> Vec<SshHostEntry> {
    let mut hosts = Vec::new();
    let mut current: Option<SshHostEntry> = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') { continue; }
        if let Some(rest) = trimmed.strip_prefix("Host ") {
            if let Some(entry) = current.take() { hosts.push(entry); }
            let pattern = rest.trim().to_string();
            current = Some(SshHostEntry { pattern, hostname: None, user: None, port: None, other: vec![] });
            continue;
        }
        if let Some(entry) = current.as_mut() {
            let mut parts = trimmed.split_whitespace();
            if let Some(key) = parts.next() {
                let value = parts.collect::<Vec<_>>().join(" ");
                let key_lower = key.to_lowercase();
                match key_lower.as_str() {
                    "hostname" => entry.hostname = Some(value),
                    "user" => entry.user = Some(value),
                    "port" => entry.port = value.parse::<u16>().ok(),
                    _ => entry.other.push((key.to_string(), value)),
                }
            }
        }
    }
    if let Some(entry) = current.take() { hosts.push(entry); }
    hosts
}


