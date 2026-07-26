use std::{
    fs::Metadata,
    io::{Read, Seek, SeekFrom},
    path::Path,
    time::SystemTime,
};

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

use crate::local_trust::open_owned_regular;

#[derive(Clone, Copy, PartialEq, Eq)]
struct FileIdentity {
    device: u64,
    inode: u64,
}

fn file_identity(metadata: &Metadata) -> Option<FileIdentity> {
    #[cfg(unix)]
    {
        Some(FileIdentity {
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        None
    }
}

pub(crate) struct AppendCursor {
    offset: u64,
    carry: Vec<u8>,
    discard_partial_line: bool,
    identity: Option<FileIdentity>,
    modified: Option<SystemTime>,
}

impl AppendCursor {
    pub(crate) fn new(offset: u64) -> Self {
        Self {
            offset,
            carry: Vec::new(),
            discard_partial_line: offset > 0,
            identity: None,
            modified: None,
        }
    }

    fn reset(&mut self) {
        self.offset = 0;
        self.carry.clear();
        self.discard_partial_line = false;
    }

    pub(crate) fn read_appended(&mut self, path: &Path) -> std::io::Result<Vec<String>> {
        self.read_appended_with_reset(path).map(|(lines, _)| lines)
    }

    pub(crate) fn read_appended_with_reset(
        &mut self,
        path: &Path,
    ) -> std::io::Result<(Vec<String>, bool)> {
        let mut file = open_owned_regular(path)?;
        let metadata = file.metadata()?;
        let size = metadata.len();
        let identity = file_identity(&metadata);
        let modified = metadata.modified().ok();
        let identity_changed = self.identity.is_some() && self.identity != identity;
        let same_size_rewrite = self.modified.is_some()
            && size == self.offset
            && modified.is_some()
            && self.modified != modified;
        let reset = identity_changed || size < self.offset || same_size_rewrite;
        if reset {
            self.reset();
        }

        if self.discard_partial_line && self.offset > 0 && self.identity.is_none() {
            file.seek(SeekFrom::Start(self.offset - 1))?;
            let mut previous = [0_u8; 1];
            file.read_exact(&mut previous)?;
            self.discard_partial_line = previous[0] != b'\n';
        }
        self.identity = identity;
        self.modified = modified;
        if size == self.offset {
            return Ok((Vec::new(), reset));
        }

        file.seek(SeekFrom::Start(self.offset))?;
        let mut bytes = Vec::new();
        file.take(size - self.offset).read_to_end(&mut bytes)?;
        self.offset += bytes.len() as u64;
        self.carry.extend(bytes);

        let mut lines = Vec::new();
        let mut consumed = 0;
        for newline in self
            .carry
            .iter()
            .enumerate()
            .filter_map(|(index, byte)| (*byte == b'\n').then_some(index))
        {
            let mut line = &self.carry[consumed..newline];
            consumed = newline + 1;
            if self.discard_partial_line {
                self.discard_partial_line = false;
                continue;
            }
            if line.last() == Some(&b'\r') {
                line = &line[..line.len() - 1];
            }
            lines.push(String::from_utf8_lossy(line).into_owned());
        }
        if consumed > 0 {
            self.carry.drain(..consumed);
        }
        Ok((lines, reset))
    }
}

#[cfg(test)]
mod tests {
    use super::AppendCursor;
    use std::{
        fs::{self, OpenOptions},
        io::Write,
        path::PathBuf,
        thread,
        time::Duration,
    };

    fn test_file(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("copets-tail-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        root.join(name)
    }

    fn append(path: &PathBuf, bytes: &[u8]) {
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap()
            .write_all(bytes)
            .unwrap();
    }

    #[test]
    fn preserves_split_utf8_until_a_complete_line_arrives() {
        let path = test_file("events.log");
        let text = "你好\n".as_bytes();
        let mut cursor = AppendCursor::new(0);
        append(&path, &text[..2]);
        assert!(cursor.read_appended(&path).unwrap().is_empty());
        append(&path, &text[2..]);
        assert_eq!(cursor.read_appended(&path).unwrap(), ["你好"]);
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn resets_after_truncation_and_same_size_rewrite() {
        let path = test_file("events.log");
        fs::write(&path, b"first-long-line\n").unwrap();
        let mut cursor = AppendCursor::new(0);
        assert_eq!(cursor.read_appended(&path).unwrap(), ["first-long-line"]);

        fs::write(&path, b"short\n").unwrap();
        assert_eq!(cursor.read_appended(&path).unwrap(), ["short"]);
        thread::sleep(Duration::from_millis(5));
        fs::write(&path, b"other\n").unwrap();
        assert_eq!(cursor.read_appended(&path).unwrap(), ["other"]);
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn resets_when_a_rotated_path_gets_a_new_file_identity() {
        let path = test_file("events.log");
        let rotated = path.with_extension("log.1");
        fs::write(&path, b"before\n").unwrap();
        let mut cursor = AppendCursor::new(0);
        assert_eq!(cursor.read_appended(&path).unwrap(), ["before"]);
        fs::rename(&path, rotated).unwrap();
        fs::write(&path, b"after\n").unwrap();
        assert_eq!(cursor.read_appended(&path).unwrap(), ["after"]);
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn initial_tail_offset_discards_the_first_partial_line() {
        let path = test_file("events.log");
        fs::write(&path, b"partial\ncomplete\n").unwrap();
        let mut cursor = AppendCursor::new(3);
        assert_eq!(cursor.read_appended(&path).unwrap(), ["complete"]);
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn initial_tail_offset_keeps_a_line_that_starts_at_the_boundary() {
        let path = test_file("events.log");
        fs::write(&path, b"a\ncomplete\n").unwrap();
        let mut cursor = AppendCursor::new(2);
        assert_eq!(cursor.read_appended(&path).unwrap(), ["complete"]);
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
