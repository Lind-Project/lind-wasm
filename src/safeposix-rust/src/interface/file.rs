// Author: Nicholas Renner
//
// File related interface
#![allow(dead_code)]

use dashmap::DashSet;
use parking_lot::Mutex;
use std::env;
pub use std::ffi::CStr as RustCStr;
use std::fs::{self, canonicalize, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
pub use std::path::{Component as RustPathComponent, Path as RustPath, PathBuf as RustPathBuf};
use std::slice;
use std::sync::Arc;
pub use std::sync::LazyLock as RustLazyGlobal;

use crate::interface::errnos::{syscall_error, Errno};
use libc::{mmap, mremap, munmap, off64_t, MAP_SHARED, MREMAP_MAYMOVE, PROT_READ, PROT_WRITE};
use std::convert::TryInto;
use std::ffi::c_void;
use std::os::unix::fs::FileExt;
use std::os::unix::io::{AsRawFd, RawFd};

pub fn removefile(filename: String) -> std::io::Result<()> {
    let path: RustPathBuf = [".".to_string(), filename].iter().collect();

    let absolute_filename = canonicalize(&path)?; //will return an error if the file does not exist

    fs::remove_file(absolute_filename)?;

    Ok(())
}

pub fn openfile(filename: String, filesize: usize) -> std::io::Result<EmulatedFile> {
    EmulatedFile::new(filename, filesize)
}

pub fn openmetadata(filename: String) -> std::io::Result<EmulatedFile> {
    EmulatedFile::new_metadata(filename)
}

#[derive(Debug)]
pub struct EmulatedFile {
    filename: String,
    fobj: Option<Arc<Mutex<File>>>,
    filesize: usize,
}

pub fn pathexists(filename: String) -> bool {
    let path: RustPathBuf = [".".to_string(), filename.clone()].iter().collect();
    path.exists()
}

impl EmulatedFile {
    fn new(filename: String, filesize: usize) -> std::io::Result<EmulatedFile> {
        let f = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(filename.clone())
            .unwrap();
        Ok(EmulatedFile {
            filename,
            fobj: Some(Arc::new(Mutex::new(f))),
            filesize,
        })
    }

    fn new_metadata(filename: String) -> std::io::Result<EmulatedFile> {
        let f = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(filename.clone())
            .unwrap();

        let filesize = f.metadata()?.len();

        Ok(EmulatedFile {
            filename,
            fobj: Some(Arc::new(Mutex::new(f))),
            filesize: filesize as usize,
        })
    }

    pub fn close(&self) -> std::io::Result<()> {
        Ok(())
    }

    pub fn shrink(&mut self, length: usize) -> std::io::Result<()> {
        if length > self.filesize {
            panic!(
                "Something is wrong. {} is already smaller than length.",
                self.filename
            );
        }
        match &self.fobj {
            None => panic!("{} is already closed.", self.filename),
            Some(f) => {
                let fobj = f.lock();
                fobj.set_len(length as u64)?;
                self.filesize = length;
                Ok(())
            }
        }
    }

    pub fn fdatasync(&self) -> std::io::Result<()> {
        match &self.fobj {
            None => panic!("{} is already closed.", self.filename),
            Some(f) => {
                let fobj = f.lock();
                fobj.sync_data()?;
                Ok(())
            }
        }
    }

    pub fn fsync(&self) -> std::io::Result<()> {
        match &self.fobj {
            None => panic!("{} is already closed.", self.filename),
            Some(f) => {
                let fobj = f.lock();
                fobj.sync_all()?;
                Ok(())
            }
        }
    }

    pub fn sync_file_range(&self, offset: isize, nbytes: isize, flags: u32) -> i32 {
        let fd = &self.as_fd_handle_raw_int();
        let valid_flags = libc::SYNC_FILE_RANGE_WAIT_BEFORE
            | libc::SYNC_FILE_RANGE_WRITE
            | libc::SYNC_FILE_RANGE_WAIT_AFTER;
        if !(flags & !valid_flags == 0) {
            return syscall_error(
                Errno::EINVAL,
                "sync_file_range",
                "flags specifies an invalid bit",
            );
        }
        unsafe { libc::sync_file_range(*fd, offset as off64_t, nbytes as off64_t, flags) }
    }

    // Wrapper around Rust's file object read_at function
    // Reads from file at specified offset into provided C-buffer
    // We need to specify the offset for read/write operations because multiple cages may refer to same system file handle
    pub fn readat(&self, ptr: *mut u8, length: usize, offset: usize) -> std::io::Result<usize> {
        let buf = unsafe {
            assert!(!ptr.is_null());
            slice::from_raw_parts_mut(ptr, length)
        };

        match &self.fobj {
            None => panic!("{} is already closed.", self.filename),
            Some(f) => {
                let fobj = f.lock();
                if offset > self.filesize {
                    panic!("Seek offset extends past the EOF!");
                }
                let bytes_read = fobj.read_at(buf, offset as u64)?;
                Ok(bytes_read)
            }
        }
    }

    // Wrapper around Rust's file object write_at function
    // Writes from provided C-buffer into file at specified offset
    // We need to specify the offset for read/write operations because multiple cages may refer to same system file handle
    pub fn writeat(
        &mut self,
        ptr: *const u8,
        length: usize,
        offset: usize,
    ) -> std::io::Result<usize> {
        let bytes_written;

        let buf = unsafe {
            assert!(!ptr.is_null());
            slice::from_raw_parts(ptr, length)
        };

        match &self.fobj {
            None => panic!("{} is already closed.", self.filename),
            Some(f) => {
                let fobj = f.lock();
                if offset > self.filesize {
                    panic!("Seek offset extends past the EOF!");
                }
                bytes_written = fobj.write_at(buf, offset as u64)?;
            }
        }

        // update our recorded filesize if we've written past the old filesize
        if offset + length > self.filesize {
            self.filesize = offset + length;
        }

        Ok(bytes_written)
    }

    // Reads entire file into bytes
    pub fn readfile_to_new_bytes(&self) -> std::io::Result<Vec<u8>> {
        match &self.fobj {
            None => panic!("{} is already closed.", self.filename),
            Some(f) => {
                let mut stringbuf = Vec::new();
                let mut fobj = f.lock();
                fobj.read_to_end(&mut stringbuf)?;
                Ok(stringbuf) // return new buf string
            }
        }
    }

    // Write to entire file from provided bytes
    pub fn writefile_from_bytes(&mut self, buf: &[u8]) -> std::io::Result<()> {
        let length = buf.len();
        let offset = self.filesize;

        match &self.fobj {
            None => panic!("{} is already closed.", self.filename),
            Some(f) => {
                let mut fobj = f.lock();
                if offset > self.filesize {
                    panic!("Seek offset extends past the EOF!");
                }
                fobj.seek(SeekFrom::Start(offset as u64))?;
                fobj.write(buf)?;
            }
        }

        if offset + length > self.filesize {
            self.filesize = offset + length;
        }

        Ok(())
    }

    pub fn zerofill_at(&mut self, offset: usize, count: usize) -> std::io::Result<usize> {
        let bytes_written;
        let buf = vec![0; count];

        match &self.fobj {
            None => panic!("{} is already closed.", self.filename),
            Some(f) => {
                let mut fobj = f.lock();
                if offset > self.filesize {
                    panic!("Seek offset extends past the EOF!");
                }
                fobj.seek(SeekFrom::Start(offset as u64))?;
                bytes_written = fobj.write(buf.as_slice())?;
            }
        }

        if offset + count > self.filesize {
            self.filesize = offset + count;
        }

        Ok(bytes_written)
    }

    //gets the raw fd handle (integer) from a rust fileobject
    pub fn as_fd_handle_raw_int(&self) -> i32 {
        if let Some(wrapped_barefile) = &self.fobj {
            wrapped_barefile.lock().as_raw_fd() as i32
        } else {
            -1
        }
    }
}

pub const COUNTMAPSIZE: usize = 8;
pub const MAP_1MB: usize = usize::pow(2, 20);

#[derive(Debug)]
pub struct EmulatedFileMap {
    filename: String,
    fobj: Arc<Mutex<File>>,
    map: Arc<Mutex<Option<Vec<u8>>>>,
    count: usize,
    countmap: Arc<Mutex<Option<Vec<u8>>>>,
    mapsize: usize,
}

pub fn mapfilenew(filename: String) -> std::io::Result<EmulatedFileMap> {
    EmulatedFileMap::new(filename)
}

impl EmulatedFileMap {
    fn new(filename: String) -> std::io::Result<EmulatedFileMap> {
        let f = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(filename.clone())
            .unwrap();

        let mapsize = MAP_1MB - COUNTMAPSIZE;
        // set the file equal to where were mapping the count and the actual map
        let _newsize = f.set_len((COUNTMAPSIZE + mapsize) as u64).unwrap();

        let map: Vec<u8>;
        let countmap: Vec<u8>;

        // here were going to map the first 8 bytes of the file as the "count" (amount of bytes written), and then map another 1MB for logging
        unsafe {
            let map_addr = mmap(
                0 as *mut c_void,
                MAP_1MB,
                PROT_READ | PROT_WRITE,
                MAP_SHARED,
                f.as_raw_fd() as i32,
                0 as i64,
            );
            countmap = Vec::<u8>::from_raw_parts(map_addr as *mut u8, COUNTMAPSIZE, COUNTMAPSIZE);
            let map_ptr = map_addr as *mut u8;
            map =
                Vec::<u8>::from_raw_parts(map_ptr.offset(COUNTMAPSIZE as isize), mapsize, mapsize);
        }

        Ok(EmulatedFileMap {
            filename,
            fobj: Arc::new(Mutex::new(f)),
            map: Arc::new(Mutex::new(Some(map))),
            count: 0,
            countmap: Arc::new(Mutex::new(Some(countmap))),
            mapsize,
        })
    }

    pub fn write_to_map(&mut self, bytes_to_write: &[u8]) -> std::io::Result<()> {
        let writelen = bytes_to_write.len();

        // if we're writing past the current map, increase the map another 1MB
        if writelen + self.count > self.mapsize {
            self.extend_map();
        }

        let mut mapopt = self.map.lock();
        let map = mapopt.as_deref_mut().unwrap();

        let mapslice = &mut map[self.count..(self.count + writelen)];
        mapslice.copy_from_slice(bytes_to_write);
        self.count += writelen;

        // update the bytes written in the map portion
        let mut countmapopt = self.countmap.lock();
        let countmap = countmapopt.as_deref_mut().unwrap();
        countmap.copy_from_slice(&self.count.to_be_bytes());

        Ok(())
    }

    fn extend_map(&mut self) {
        // open count and map to resize mmap, and file to increase file size
        let mut mapopt = self.map.lock();
        let map = mapopt.take().unwrap();
        let mut countmapopt = self.countmap.lock();
        let countmap = countmapopt.take().unwrap();
        let f = self.fobj.lock();

        // add another 1MB to mapsize
        let new_mapsize = self.mapsize + MAP_1MB;
        let _newsize = f.set_len((COUNTMAPSIZE + new_mapsize) as u64).unwrap();

        let newmap: Vec<u8>;
        let newcountmap: Vec<u8>;

        // destruct count and map and re-map
        unsafe {
            let (old_count_map_addr, countlen, _countcap) = countmap.into_raw_parts();
            assert_eq!(COUNTMAPSIZE, countlen);
            let (_old_map_addr, len, _cap) = map.into_raw_parts();
            assert_eq!(self.mapsize, len);
            let map_addr = mremap(
                old_count_map_addr as *mut c_void,
                COUNTMAPSIZE + self.mapsize,
                COUNTMAPSIZE + new_mapsize,
                MREMAP_MAYMOVE,
            );

            newcountmap =
                Vec::<u8>::from_raw_parts(map_addr as *mut u8, COUNTMAPSIZE, COUNTMAPSIZE);
            let map_ptr = map_addr as *mut u8;
            newmap = Vec::<u8>::from_raw_parts(
                map_ptr.offset(COUNTMAPSIZE as isize),
                new_mapsize,
                new_mapsize,
            );
        }

        // replace maps
        mapopt.replace(newmap);
        countmapopt.replace(newcountmap);
        self.mapsize = new_mapsize;
    }

    pub fn close(&self) -> std::io::Result<()> {
        let mut mapopt = self.map.lock();
        let map = mapopt.take().unwrap();
        let mut countmapopt = self.countmap.lock();
        let countmap = countmapopt.take().unwrap();

        unsafe {
            let (countmap_addr, countlen, _countcap) = countmap.into_raw_parts();
            assert_eq!(COUNTMAPSIZE, countlen);
            munmap(countmap_addr as *mut c_void, COUNTMAPSIZE);

            let (map_addr, len, _cap) = map.into_raw_parts();
            assert_eq!(self.mapsize, len);
            munmap(map_addr as *mut c_void, self.mapsize);
        }

        Ok(())
    }
}

#[derive(Debug)]
pub struct ShmFile {
    fobj: Arc<Mutex<File>>,
    key: i32,
    size: usize,
}

pub fn new_shm_backing(key: i32, size: usize) -> std::io::Result<ShmFile> {
    ShmFile::new(key, size)
}

// Mimic shared memory in Linux by creating a file backing and truncating it to the segment size
// We can then safely unlink the file while still holding a descriptor to that segment,
// which we can use to map shared across cages.
impl ShmFile {
    fn new(key: i32, size: usize) -> std::io::Result<ShmFile> {
        // open file "shm-#id"
        let filename = format!("{}{}", "shm-", key);
        let f = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(filename.clone())
            .unwrap();
        // truncate file to size
        f.set_len(size as u64)?;
        // unlink file
        fs::remove_file(filename)?;
        let shmfile = ShmFile {
            fobj: Arc::new(Mutex::new(f)),
            key,
            size,
        };

        Ok(shmfile)
    }

    //gets the raw fd handle (integer) from a rust fileobject
    pub fn as_fd_handle_raw_int(&self) -> i32 {
        self.fobj.lock().as_raw_fd() as i32
    }
}

// convert a series of big endian bytes to a size
pub fn convert_bytes_to_size(bytes_to_write: &[u8]) -> usize {
    let sizearray: [u8; 8] = bytes_to_write.try_into().unwrap();
    usize::from_be_bytes(sizearray)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_path_exists_true() {
        let temp_file = NamedTempFile::new().unwrap();
        let file_path = temp_file.path().to_str().unwrap().to_string();
        assert!(pathexists(file_path));
    }

    #[test]
    fn test_path_exists_false() {
        // Test that pathexists returns false for a non-existent file
        let non_existent_file = "/tmp/non_existent_file.txt";
        assert!(!pathexists(non_existent_file.to_string()));
    }
    #[test]
    fn test_new_emulated_file() {
        let filename = "test_file.txt";
        let filesize = 1024;

        let emulated_file = EmulatedFile::new(filename.to_string(), filesize).unwrap();

        assert_eq!(emulated_file.filename, filename);
        assert_eq!(emulated_file.filesize, filesize);
        assert!(emulated_file.fobj.is_some());
    }

    #[test]
    fn test_new_metadata_emulated_file() {
        let temp_file = NamedTempFile::new().unwrap();
        let file_path = temp_file.path().to_str().unwrap().to_string();

        let emulated_file = EmulatedFile::new_metadata(file_path.clone()).unwrap();

        assert_eq!(emulated_file.filename, file_path);
        assert!(emulated_file.fobj.is_some());
    }
    #[test]
    fn test_readat_emulated_file() {
        let temp_file = NamedTempFile::new().unwrap();
        let file_path = temp_file.path().to_str().unwrap().to_string();
        let file_content = b"Hello, world!";
        temp_file.as_file().write_all(file_content).unwrap();

        let emulated_file = EmulatedFile::new(file_path.clone(), file_content.len()).unwrap();

        let mut buffer = vec![0; file_content.len()];
        let bytes_read = emulated_file
            .readat(buffer.as_mut_ptr(), buffer.len(), 0)
            .unwrap();

        assert_eq!(bytes_read, file_content.len());
        assert_eq!(buffer, file_content);
    }

    #[test]
    fn test_writeat_emulated_file() {
        let temp_file = NamedTempFile::new().unwrap();
        let file_path = temp_file.path().to_str().unwrap().to_string();
        let file_content = b"Hello, world!";

        let mut emulated_file = EmulatedFile::new(file_path.clone(), file_content.len()).unwrap();

        let new_content = b"test_writeat_emulated_file, world!";
        let bytes_written = emulated_file
            .writeat(new_content.as_ptr(), new_content.len(), 0)
            .unwrap();

        assert_eq!(bytes_written, new_content.len());
        assert_eq!(emulated_file.filesize, new_content.len());

        let mut buffer = vec![0; new_content.len()];
        emulated_file
            .readat(buffer.as_mut_ptr(), buffer.len(), 0)
            .unwrap();
        assert_eq!(buffer, new_content);
    }
}
