//! Implements a writable instance of a 7zip File.
//!
//! This implementation is writing the data into the 7zip file right away and
//! will finish the file on drop.

use std::collections::HashSet;
use std::fs::File;
use std::io::BufWriter;
use std::num::NonZeroUsize;
use std::path::Path;

use sevenz_rust2::encoder_options::LZMA2Options;
use sevenz_rust2::{ArchiveEntry, ArchiveWriter, EncoderMethod, NtTime};

use super::SevenZipArchive;
use crate::loaders::archive::{Archive, Compression, Writable};

pub struct SevenZipArchiveBuilder {
    writer: Option<ArchiveWriter<BufWriter<File>>>,
    seen_directories: HashSet<String>,
    thread_count: u32,
}

impl SevenZipArchiveBuilder {
    pub fn from_path(path: &Path) -> Self {
        let file = File::create(path).expect("can't create archive file");
        let writer = ArchiveWriter::new(BufWriter::new(file)).unwrap();

        let thread_count = std::thread::available_parallelism().unwrap_or(NonZeroUsize::new(1).unwrap()).get() as u32;

        Self {
            writer: Some(writer),
            seen_directories: HashSet::default(),
            thread_count,
        }
    }

    #[must_use]
    pub fn copy_file_from_archive(&mut self, archive: &SevenZipArchive, path: &str) -> bool {
        let Some(mut compression) = archive.file_is_compressed(path) else {
            return false;
        };

        let Some(data) = archive.get_file_by_path(path) else {
            return false;
        };

        let path_with_slash = path.replace('\\', "/").to_string();

        get_parent_directories(&path_with_slash)
            .iter()
            .for_each(|directory| self.add_directory(directory));

        // Custom overrides if we want to use different compressions on re-sync in
        // future versions.
        if path.ends_with(".dds") {
            compression = Compression::Off;
        }

        self.add_file(path, data, compression);

        true
    }

    fn add_directory(&mut self, path: &str) {
        if !self.seen_directories.contains(path) {
            self.seen_directories.insert(path.to_string());
        }
    }
}

impl Writable for SevenZipArchiveBuilder {
    fn add_file(&mut self, path: &str, asset_data: Vec<u8>, compression: Compression) {
        let path = path.replace('\\', "/").to_string();

        get_parent_directories(&path)
            .iter()
            .for_each(|directory| self.add_directory(directory));

        if let Some(writer) = self.writer.as_mut() {
            let mut file_entry = ArchiveEntry::new_file(&path);

            let now = NtTime::now();

            file_entry.creation_date = now;
            file_entry.has_creation_date = true;
            file_entry.last_modified_date = now;
            file_entry.has_last_modified_date = true;
            file_entry.has_access_date = true;
            file_entry.access_date = now;

            // Since we want to enable multithreaded decoding, we will slice the file when
            // compressing. This is a LZMA2 feature to enable multithreaded decompression.
            // The smaller we make this, the less efficient the compression
            // ratio is, but the better the parallelization can be.
            let stream_size = file_entry.size / 16;

            match compression {
                Compression::Off => writer.set_content_methods(vec![EncoderMethod::COPY.into()]),
                Compression::Default => {
                    writer.set_content_methods(vec![LZMA2Options::from_level_mt(7, self.thread_count, stream_size).into()])
                }
            };

            writer
                .push_archive_entry(file_entry, Some(asset_data.as_slice()))
                .expect("failed to write file to archive");
        }
    }

    fn finish(&mut self) -> Result<(), std::io::Error> {
        // File will be finished on drop
        Ok(())
    }
}

impl Drop for SevenZipArchiveBuilder {
    fn drop(&mut self) {
        if let Some(mut writer) = self.writer.take() {
            let mut seen_directories: Vec<String> = self.seen_directories.drain().collect();
            seen_directories.sort();

            for directory_path in seen_directories.iter() {
                let directory = ArchiveEntry::new_directory(directory_path);
                writer
                    .push_archive_entry::<&[u8]>(directory, None)
                    .unwrap_or_else(|_| panic!("can't add directory path '{directory_path}' to archive"));
            }

            writer.finish().unwrap();
        }
    }
}

fn get_parent_directories(asset_path: &str) -> Vec<String> {
    let mut result = Vec::new();
    let parts: Vec<&str> = asset_path.split('/').collect();

    for index in 1..parts.len() {
        let path = parts[..index].join("/");
        if !path.is_empty() {
            result.push(path);
        }
    }

    result
}
