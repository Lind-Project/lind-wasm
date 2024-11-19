#![allow(dead_code)]

// File system related system calls
use super::fs_constants::*;
use super::sys_constants::*;
use crate::interface;
use crate::safeposix::cage::Errno::EINVAL;
use crate::safeposix::cage::{FileDescriptor::*, *};
use crate::safeposix::filesystem::*;
use crate::safeposix::net::NET_METADATA;
use crate::safeposix::shm::*;

impl Cage {
    /// ## ------------------OPEN SYSCALL------------------
    /// ### Description
    /// The `open_syscall()` creates an open file description that refers to a file and a file descriptor that refers to that open file description.
    /// The file descriptor is used by other I/O functions to refer to that file.
    /// There are generally two cases which occur when this function is called. 
    /// Case 1: If the file to be opened doesn't exist, then a new file is created at the given location and a new file descriptor is created.
    /// Case 2: If the file already exists, then a few conditions are checked and based on them, file is updated accordingly.
    
    /// ### Function Arguments
    /// The `open_syscall()` receives three arguments:
    /// * `path` - This argument points to a pathname naming the file.
    ///           For example: "/parentdir/file1" represents a file which will be either opened if exists or will be created at the given path.
    /// * `flags` - This argument contains the file status flags and file access modes which will be alloted to the open file description.
    ///            The flags are combined together using a bitwise-inclusive-OR and the result is passed as an argument to the function.
    ///            Some of the most common flags used are: O_CREAT | O_TRUNC | O_RDWR | O_EXCL | O_RDONLY | O_WRONLY, with each representing a different file mode.
    /// * `mode` - This represents the permission of the newly created file. 
    ///           The general mode used is "S_IRWXA": which represents the read, write, and search permissions on the new file. 
    
    /// ### Returns
    /// Upon successful completion of this call, a file descriptor is returned which points the file which is opened.
    /// Otherwise, errors or panics are returned for different scenarios.
    ///
    /// ### Errors and Panics
    /// * ENFILE - no available file descriptor number could be found
    /// * ENOENT - tried to open a file that did not exist
    /// * EINVAL - the input flags contain S_IFCHR flag representing a special character file
    /// * EPERM - the mode bits for a file are not sane
    /// * ENOTDIR - tried to create a file as a child of something that isn't a directory
    /// * EEXIST - the file already exists and O_CREAT and O_EXCL flags were passed
    /// * ENXIO - the file is of type UNIX domain socket
    /// 
    /// A panic occurs when there is some issue fetching the file descriptor.
    /// 
    /// for more detailed description of all the commands and return values, see 
    /// [open(2)](https://man7.org/linux/man-pages/man2/open.2.html)
    ///

    // This function is used to create a new File Descriptor Object and return it.
    // This file descriptor object is then inserted into the File Descriptor Table of the associated cage in the open_syscall() function
    fn _file_initializer(&self, inodenum: usize, flags: i32, size: usize) -> FileDesc {
        let position = if 0 != flags & O_APPEND { size } else { 0 };

        // While creating a new FileDescriptor, there are two important things that need to be present:
        // O_RDWRFLAGS:- This flag determines whether the file is opened for reading, writing, or both.
        // O_CLOEXEC - This flag indicates that the file descriptor should be automatically closed during an exec family function. 
        // It’s needed for managing file descriptors across different processes, ensuring that they do not unintentionally remain open.
        let allowmask = O_RDWRFLAGS | O_CLOEXEC;
        FileDesc {
            position: position,
            inode: inodenum,
            flags: flags & allowmask,
            advlock: interface::RustRfc::new(interface::AdvisoryLock::new()),
        }
    }

    pub fn open_syscall(&self, path: &str, flags: i32, mode: u32) -> i32 {
        // Check that the given input path is not empty
        if path.len() == 0 {
            return syscall_error(Errno::ENOENT, "open", "given path was null");
        }
        
        // Retrieve the absolute path from the root directory. The absolute path is then used to validate directory paths
        // while navigating through subdirectories and creating a new file or open existing file at the given location.
        let truepath = normpath(convpath(path), self);

        // Fetch the next file descriptor and its lock write guard to ensure the file can be associated with the file descriptor 
        let (fd, guardopt) = self.get_next_fd(None);
        match fd {
            // If the file descriptor is invalid, the return value is always an error with value (ENFILE).
            fd if fd == (Errno::ENFILE as i32) => {
                return syscall_error(
                    Errno::ENFILE,
                    "open_helper",
                    "no available file descriptor number could be found",
                );
            },
            // When the file descriptor is valid, we proceed with performing the remaining checks for open_syscall.
            fd if fd > 0 => {
                // File Descriptor Write Lock Guard
                let fdoption = &mut *guardopt.unwrap();

                // Walk through the absolute path which returns a tuple consisting of inode number of file (if it exists), and inode number of parent (if it exists)
                match metawalkandparent(truepath.as_path()) {
                    // Case 1: When the file doesn't exist but the parent directory exists
                    (None, Some(pardirinode)) => {
                        // Check if O_CREAT flag is not present, then a file can not be created and error is returned.
                        if 0 == (flags & O_CREAT) {
                            return syscall_error(
                                Errno::ENOENT,
                                "open",
                                "tried to open a file that did not exist, and O_CREAT was not specified",
                            );
                        }

                        // Error is thrown when the input flags contain S_IFCHR flag representing a special character file.
                        if S_IFCHR == (S_IFCHR & flags) {
                            return syscall_error(Errno::EINVAL, "open", "Invalid value in flags");
                        }

                        // S_FILETYPEFLAGS represents a bitmask that can be used to extract the file type information from a file's mode.
                        // This code is referenced from Lind-Repy codebase.
                        // Here, we are checking whether the mode bits are sane by ensuring that only valid file permission bits (S_IRWXA) and file type bits (S_FILETYPEFLAGS) are set. Else, we return the error.
                        if mode & (S_IRWXA | S_FILETYPEFLAGS as u32) != mode {
                            return syscall_error(Errno::EPERM, "open", "Mode bits were not sane");
                        }

                        let filename = truepath.file_name().unwrap().to_str().unwrap().to_string(); //for now we assume this is sane, but maybe this should be checked later
                        let time = interface::timestamp(); //We do a real timestamp now

                        // S_IFREG is the flag for a regular file, so it's added to the mode to indicate that the new file being created is a regular file.
                        let effective_mode = S_IFREG as u32 | mode;
                        
                        // Create a new inode of type "File" representing a file and set the required attributes
                        let newinode = Inode::File(GenericInode {
                            size: 0, 
                            uid: DEFAULT_UID,
                            gid: DEFAULT_GID,
                            mode: effective_mode,
                            linkcount: 1, // because when a new file is created, it has a single hard link, which is the directory entry that points to this file's inode.
                            refcount: 1, // Because a new file descriptor will open and refer to this file
                            atime: time,
                            ctime: time,
                            mtime: time,
                        });

                        // Fetch the next available inode number using the FileSystem MetaData table
                        let newinodenum = FS_METADATA
                            .nextinode
                            .fetch_add(1, interface::RustAtomicOrdering::Relaxed); //fetch_add returns the previous value, which is the inode number we want
                        
                        // Fetch the inode of the parent directory and only proceed when its type is directory.
                        if let Inode::Dir(ref mut ind) = *(FS_METADATA.inodetable.get_mut(&pardirinode).unwrap())
                        {
                            ind.filename_to_inode_dict.insert(filename, newinodenum);
                            ind.linkcount += 1; // Since the parent is now associated to the new file, its linkcount will increment by 1
                            ind.ctime = time; // Here, update the ctime and mtime for the parent directory as well
                            ind.mtime = time;
                        } else {
                            return syscall_error(
                                Errno::ENOTDIR,
                                "open",
                                "tried to create a file as a child of something that isn't a directory",
                            );
                        }
                        // Update the inode table by inserting the newly formed inode mapped with its inode number.
                        FS_METADATA.inodetable.insert(newinodenum, newinode);
                        log_metadata(&FS_METADATA, pardirinode);
                        log_metadata(&FS_METADATA, newinodenum);

                        // FileObjectTable stores the entries of the currently opened files in the system
                        // Since, a new file is being opened here, an entry corresponding to that newinode is made in the FileObjectTable
                        // An entry in the table has the following representation:
                        // Key - inode number
                        // Value - Opened file with its size as 0
                        if let interface::RustHashEntry::Vacant(vac) = FILEOBJECTTABLE.entry(newinodenum) {
                            let sysfilename = format!("{}{}", FILEDATAPREFIX, newinodenum);
                            vac.insert(interface::openfile(sysfilename, 0).unwrap()); // new file of size 0
                        }

                        // The file object of size 0, associated with the newinode number is inserted into the FileDescriptorTable associated with the cage using the guard lock.
                        let _insertval =
                            fdoption.insert(File(self._file_initializer(newinodenum, flags, 0)));
                    }

                    // Case 2: When the file exists (we don't need to look at parent here)
                    (Some(inodenum), ..) => {
                        //If O_CREAT and O_EXCL flags are set in the input parameters, open_syscall() fails if the file exists. 
                        //This is because the check for the existence of the file and the creation of the file if it does not exist is atomic, 
                        //with respect to other threads executing open() naming the same filename in the same directory with O_EXCL and O_CREAT set.
                        if (O_CREAT | O_EXCL) == (flags & (O_CREAT | O_EXCL)) {
                            return syscall_error(
                                Errno::EEXIST,
                                "open",
                                "file already exists and O_CREAT and O_EXCL were used",
                            );
                        }
                        let size;

                        // Fetch the Inode Object associated with the inode number of the existing file.
                        // There are different Inode types supported by the open_syscall (i.e., File, Directory, Socket, CharDev).
                        let mut inodeobj = FS_METADATA.inodetable.get_mut(&inodenum).unwrap();
                        match *inodeobj {
                            Inode::File(ref mut f) => {
                                //This is a special case when the input flags contain "O_TRUNC" flag, 
                                //This flag truncates the file size to 0, and the mode and owner are unchanged
                                // and is only used when the file exists and is a regular file
                                if O_TRUNC == (flags & O_TRUNC) {
                                    // Close the existing file object and remove it from the FileObject Hashtable using the inodenumber
                                    let entry = FILEOBJECTTABLE.entry(inodenum);
                                    if let interface::RustHashEntry::Occupied(occ) = &entry {
                                        occ.get().close().unwrap();
                                    }

                                    f.size = 0;

                                    // Update the timestamps as well
                                    let latest_time = interface::timestamp(); 
                                    f.ctime = latest_time;
                                    f.mtime = latest_time; 

                                    // Remove the previous file and add a new one of 0 length
                                    if let interface::RustHashEntry::Occupied(occ) = entry {
                                        occ.remove_entry();
                                    }

                                    // The current file is removed from the filesystem
                                    let sysfilename = format!("{}{}", FILEDATAPREFIX, inodenum);
                                    interface::removefile(sysfilename.clone()).unwrap();
                                }

                                // Once the metadata for the file is reset, a new file is inserted in file system.
                                // Also, it is inserted back to the FileObjectTable and associated with same inodeNumber representing that the file is currently in open state.
                                if let interface::RustHashEntry::Vacant(vac) = FILEOBJECTTABLE.entry(inodenum)
                                {
                                    let sysfilename = format!("{}{}", FILEDATAPREFIX, inodenum);
                                    vac.insert(interface::openfile(sysfilename, f.size).unwrap());
                                }

                                // Update the final size and reference count for the file 
                                size = f.size;
                                f.refcount += 1;

                                // Current Implementation for File Truncate: The previous entry of the file is removed from the FileObjectTable, with a new file of size 0 inserted back into the table.
                                // Possible Bug: Why are we not simply adjusting the file size and pointer of the existing file?
                            }

                            // When the existing file type is of Directory or Character Device, only the file size and the reference count is updated.
                            Inode::Dir(ref mut f) => {
                                size = f.size;
                                f.refcount += 1;
                            }
                            Inode::CharDev(ref mut f) => {
                                size = f.size;
                                f.refcount += 1;
                            }

                            // If the existing file type is a socket, error is thrown as socket type files are not supported by open_syscall
                            Inode::Socket(_) => {
                                return syscall_error(Errno::ENXIO, "open", "file is a UNIX domain socket");
                            }
                        }

                        // The file object of size 0, associated with the existing inode number is inserted into the FileDescriptorTable associated with the cage using the guard lock.
                        let _insertval =
                            fdoption.insert(File(self._file_initializer(inodenum, flags, size)));
                    }

                    // Case 3: When neither the file directory nor the parent directory exists
                    (None, None) => {
                        // O_CREAT flag is used to create a file if it doesn't exist.
                        // If this flag is not present, then a file can not be created and error is returned.
                        if 0 == (flags & O_CREAT) {
                            return syscall_error(
                                Errno::ENOENT,
                                "open",
                                "tried to open a file that did not exist, and O_CREAT was not specified",
                            );
                        }
                        // O_CREAT flag is set but the path doesn't exist, so return an error with a different message string.
                        return syscall_error(Errno::ENOENT, "open", "a directory component in pathname does not exist or is a dangling symbolic link");
                    }
                }

                // Once all the updates are done, the file descriptor value is returned
                fd 
            },
            // Panic when there is some other issue fetching the file descriptor.
            _ => {
                panic!("File descriptor couldn't be fetched!");
            }
        }
    }

    //------------------MKDIR SYSCALL------------------
    // Description
    // The mkdir_syscall() creates a new directory named by the path name pointed to by a path as the input parameter in the function.
    // The mode of the new directory is initialized from the "mode" provided as the input parameter in the function.
    // The newly created directory is empty with size 0 and is associated with a new inode of type "DIR".
    // On successful completion, the timestamps for both the newly formed directory and its parent are updated along with their linkcounts.
    
    // Function Arguments
    // The mkdir_syscall() receives two arguments:
    // 1. Path - This represents the path at which the new directory will be created.
    //           For example: "/parentdir/dir" represents the new directory name as "dir", which will be created at this path (/parentdir/dir).
    // 2. Mode - This represents the permission of the newly created directory. 
    //           The general mode used is "S_IRWXA": which represents the read, write, and search permissions on the new directory. 
    
    // Return Values
    // Upon successful creation of the directory, 0 is returned.
    // Otherwise, an error with a proper errorNumber and errorMessage is returned based on the different scenarios.
    //
    // Tests
    // All the different scenarios for mkdir_syscall() are covered and tested in the "fs_tests.rs" file under "mkdir_syscall_tests" section.
    //
    pub fn mkdir_syscall(&self, path: &str, mode: u32) -> i32 {

        // Check that the given input path is not empty
        if path.len() == 0 {
            return syscall_error(Errno::ENOENT, "mkdir", "given path was null");
        }

        // Store the FileMetadata into a helper variable which is used for fetching the metadata of a given inode from the Inode Table. 
        let metadata = &FS_METADATA;

        // Retrieve the absolute path from the root directory. The absolute path is then used to validate directory paths
        // while navigating through subdirectories and establishing new directory at the given location.
        let truepath = normpath(convpath(path), self);

        // Walk through the absolute path which returns a tuple consisting of inode number of file (if it exists), and inode number of parent (if it exists)
        match metawalkandparent(truepath.as_path()) {
            // Case 1: When neither the file directory nor the parent directory exists
            (None, None) => syscall_error(
                Errno::ENOENT,
                "mkdir",
                "a directory component in pathname does not exist or is a dangling symbolic link",
            ),

            // Case 2: When the file doesn't exist but the parent directory exists
            (None, Some(pardirinode)) => {
                let filename = truepath.file_name().unwrap().to_str().unwrap().to_string(); //for now we assume this is sane, but maybe this should be checked later

                let effective_mode = S_IFDIR as u32 | mode;
                // Check for the condition if the mode bits are correct and have the required permissions to create a directory
                if mode & (S_IRWXA | S_FILETYPEFLAGS as u32) != mode {
                    return syscall_error(Errno::EPERM, "mkdir", "Mode bits were not sane");
                }

                // Fetch the next available inode number using the FileSystem MetaData table
                // Create a new inode of type "Dir" representing a directory and set the required attributes
                let newinodenum = FS_METADATA
                    .nextinode
                    .fetch_add(1, interface::RustAtomicOrdering::Relaxed); //fetch_add returns the previous value, which is the inode number we want
                let time = interface::timestamp(); //We do a real timestamp now
                let newinode = Inode::Dir(DirectoryInode {
                    size: 0, //initial size of a directory is 0 as it is empty
                    uid: DEFAULT_UID,
                    gid: DEFAULT_GID,
                    mode: effective_mode,
                    linkcount: 3, //because of the directory name(.), itself, and reference to the parent directory(..)
                    refcount: 0, //because no file descriptors are pointing to it currently
                    atime: time,
                    ctime: time,
                    mtime: time,
                    filename_to_inode_dict: init_filename_to_inode_dict(newinodenum, pardirinode), //Establish a mapping between the newly created inode and the parent directory inode for easy retrieval and linking
                });

                // Insert a reference to the file in the parent directory and update the inode attributes
                // Fetch the inode of the parent directory and only proceed when its type is directory.
                if let Inode::Dir(ref mut parentdir) = *(metadata.inodetable.get_mut(&pardirinode).unwrap())
                {
                    parentdir
                        .filename_to_inode_dict
                        .insert(filename, newinodenum);
                    parentdir.linkcount += 1; // Since the parent is now associated to the new directory, its linkcount will increment by 1
                    parentdir.ctime = time; // Here, update the ctime and mtime for the parent directory as well
                    parentdir.mtime = time;
                }
                else {
                    unreachable!();
                }
                // Update the inode table by inserting the newly formed inode mapped with its inode number.
                metadata.inodetable.insert(newinodenum, newinode);
                log_metadata(&metadata, pardirinode);
                log_metadata(&metadata, newinodenum);

                // Return 0 when mkdir has succeeded
                0 
            }

            // Case 3: When the file directory name already exists, then return the error.
            (Some(_), ..) => syscall_error(
                Errno::EEXIST,
                "mkdir",
                "pathname already exists, cannot create directory",
            ),
        }
    }

    //------------------MKNOD SYSCALL------------------

    pub fn mknod_syscall(&self, path: &str, mode: u32, dev: u64) -> i32 {
        //Check that path is not empty
        if path.len() == 0 {
            return syscall_error(Errno::ENOENT, "mknod", "given path was null");
        }
        let truepath = normpath(convpath(path), self);

        //pass the metadata to this helper. If passed table is none, then create new instance
        let metadata = &FS_METADATA;

        match metawalkandparent(truepath.as_path()) {
            //If neither the file nor parent exists
            (None, None) => syscall_error(
                Errno::ENOENT,
                "mknod",
                "a directory component in pathname does not exist or is a dangling symbolic link",
            ),

            //If the file doesn't exist but the parent does
            (None, Some(pardirinode)) => {
                let filename = truepath.file_name().unwrap().to_str().unwrap().to_string(); //for now we assume this is sane, but maybe this should be checked later

                //assert sane mode bits (asserting that the mode bits make sense)
                if mode & (S_IRWXA | S_FILETYPEFLAGS as u32) != mode {
                    return syscall_error(Errno::EPERM, "mknod", "Mode bits were not sane");
                }
                if mode as i32 & S_IFCHR == 0 {
                    return syscall_error(
                        Errno::EINVAL,
                        "mknod",
                        "only character files are supported",
                    );
                }
                let time = interface::timestamp(); //We do a real timestamp now
                let newinode = Inode::CharDev(DeviceInode {
                    size: 0,
                    uid: DEFAULT_UID,
                    gid: DEFAULT_GID,
                    mode: mode,
                    linkcount: 1,
                    refcount: 0,
                    atime: time,
                    ctime: time,
                    mtime: time,
                    dev: devtuple(dev),
                });

                let newinodenum = FS_METADATA
                    .nextinode
                    .fetch_add(1, interface::RustAtomicOrdering::Relaxed); //fetch_add returns the previous value, which is the inode number we want
                if let Inode::Dir(ref mut parentdir) =
                    *(FS_METADATA.inodetable.get_mut(&pardirinode).unwrap())
                {
                    parentdir
                        .filename_to_inode_dict
                        .insert(filename, newinodenum);
                    parentdir.linkcount += 1;
                } //insert a reference to the file in the parent directory
                metadata.inodetable.insert(newinodenum, newinode);
                log_metadata(metadata, pardirinode);
                log_metadata(metadata, newinodenum);
                0 //mknod has succeeded
            }

            (Some(_), ..) => syscall_error(
                Errno::EEXIST,
                "mknod",
                "pathname already exists, cannot create device file",
            ),
        }
    }

    //------------------------------------LINK SYSCALL------------------------------------

    pub fn link_syscall(&self, oldpath: &str, newpath: &str) -> i32 {
        if oldpath.len() == 0 {
            return syscall_error(Errno::ENOENT, "link", "given oldpath was null");
        }
        if newpath.len() == 0 {
            return syscall_error(Errno::ENOENT, "link", "given newpath was null");
        }
        let trueoldpath = normpath(convpath(oldpath), self);
        let truenewpath = normpath(convpath(newpath), self);
        let filename = truenewpath
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string(); //for now we assume this is sane, but maybe this should be checked later

        match metawalk(trueoldpath.as_path()) {
            //If neither the file nor parent exists
            None => syscall_error(
                Errno::ENOENT,
                "link",
                "a directory component in pathname does not exist or is a dangling symbolic link",
            ),
            Some(inodenum) => {
                let mut inodeobj = FS_METADATA.inodetable.get_mut(&inodenum).unwrap();

                match *inodeobj {
                    Inode::File(ref mut normalfile_inode_obj) => {
                        normalfile_inode_obj.linkcount += 1; //add link to inode
                    }

                    Inode::CharDev(ref mut chardev_inode_obj) => {
                        chardev_inode_obj.linkcount += 1; //add link to inode
                    }

                    Inode::Socket(ref mut socket_inode_obj) => {
                        socket_inode_obj.linkcount += 1; //add link to inode
                    }

                    Inode::Dir(_) => {
                        return syscall_error(Errno::EPERM, "link", "oldpath is a directory")
                    }
                }

                drop(inodeobj);

                let retval = match metawalkandparent(truenewpath.as_path()) {
                    (None, None) => {
                        syscall_error(Errno::ENOENT, "link", "newpath cannot be created")
                    }

                    (None, Some(pardirinode)) => {
                        let mut parentinodeobj =
                            FS_METADATA.inodetable.get_mut(&pardirinode).unwrap();
                        //insert a reference to the inode in the parent directory
                        if let Inode::Dir(ref mut parentdirinodeobj) = *parentinodeobj {
                            parentdirinodeobj
                                .filename_to_inode_dict
                                .insert(filename, inodenum);
                            parentdirinodeobj.linkcount += 1;
                            drop(parentinodeobj);
                            log_metadata(&FS_METADATA, pardirinode);
                            log_metadata(&FS_METADATA, inodenum);
                        } else {
                            panic!("Parent directory was not a directory!");
                        }
                        0 //link has succeeded
                    }

                    (Some(_), ..) => syscall_error(Errno::EEXIST, "link", "newpath already exists"),
                };

                if retval != 0 {
                    //reduce the linkcount to its previous value if linking failed
                    let mut inodeobj = FS_METADATA.inodetable.get_mut(&inodenum).unwrap();

                    match *inodeobj {
                        Inode::File(ref mut normalfile_inode_obj) => {
                            normalfile_inode_obj.linkcount -= 1;
                        }

                        Inode::CharDev(ref mut chardev_inode_obj) => {
                            chardev_inode_obj.linkcount -= 1;
                        }

                        Inode::Socket(ref mut socket_inode_obj) => {
                            socket_inode_obj.linkcount -= 1;
                        }

                        Inode::Dir(_) => {
                            panic!("Known non-directory file has been replaced with a directory!");
                        }
                    }
                }

                return retval;
            }
        }
    }

    //------------------------------------UNLINK SYSCALL------------------------------------

    pub fn unlink_syscall(&self, path: &str) -> i32 {
        if path.len() == 0 {
            return syscall_error(Errno::ENOENT, "unmknod", "given oldpath was null");
        }
        let truepath = normpath(convpath(path), self);

        match metawalkandparent(truepath.as_path()) {
            //If the file does not exist
            (None, ..) => syscall_error(Errno::ENOENT, "unlink", "path does not exist"),

            //If the file exists but has no parent, it's the root directory
            (Some(_), None) => {
                syscall_error(Errno::EISDIR, "unlink", "cannot unlink root directory")
            }

            //If both the file and the parent directory exists
            (Some(inodenum), Some(parentinodenum)) => {
                let mut inodeobj = FS_METADATA.inodetable.get_mut(&inodenum).unwrap();

                let (currefcount, curlinkcount, has_fobj, log) = match *inodeobj {
                    Inode::File(ref mut f) => {
                        f.linkcount -= 1;
                        (f.refcount, f.linkcount, true, true)
                    }
                    Inode::CharDev(ref mut f) => {
                        f.linkcount -= 1;
                        (f.refcount, f.linkcount, false, true)
                    }
                    Inode::Socket(ref mut f) => {
                        f.linkcount -= 1;
                        (f.refcount, f.linkcount, false, false)
                    }
                    Inode::Dir(_) => {
                        return syscall_error(Errno::EISDIR, "unlink", "cannot unlink directory");
                    }
                }; //count current number of links and references

                drop(inodeobj);

                let removal_result = Self::remove_from_parent_dir(parentinodenum, &truepath);
                if removal_result != 0 {
                    return removal_result;
                }

                if curlinkcount == 0 {
                    if currefcount == 0 {
                        //actually remove file and the handle to it
                        FS_METADATA.inodetable.remove(&inodenum);
                        if has_fobj {
                            let sysfilename = format!("{}{}", FILEDATAPREFIX, inodenum);
                            interface::removefile(sysfilename).unwrap();
                        }
                    } //we don't need a separate unlinked flag, we can just check that refcount is 0
                }
                NET_METADATA.domsock_paths.remove(&truepath);

                // the log boolean will be false if we are workign on a domain socket
                if log {
                    log_metadata(&FS_METADATA, parentinodenum);
                    log_metadata(&FS_METADATA, inodenum);
                }
                0 //unlink has succeeded
            }
        }
    }

    //------------------------------------CREAT SYSCALL------------------------------------

    pub fn creat_syscall(&self, path: &str, mode: u32) -> i32 {
        self.open_syscall(path, O_CREAT | O_TRUNC | O_WRONLY, mode)
    }

    //------------------------------------STAT SYSCALL------------------------------------

    pub fn stat_syscall(&self, path: &str, statbuf: &mut StatData) -> i32 {
        let truepath = normpath(convpath(path), self);

        //Walk the file tree to get inode from path
        if let Some(inodenum) = metawalk(truepath.as_path()) {
            let inodeobj = FS_METADATA.inodetable.get(&inodenum).unwrap();

            //populate those fields in statbuf which depend on things other than the inode object
            statbuf.st_dev = FS_METADATA.dev_id;
            statbuf.st_ino = inodenum;

            //delegate the rest of populating statbuf to the relevant helper
            match &*inodeobj {
                Inode::File(f) => {
                    Self::_istat_helper(&f, statbuf);
                }
                Inode::CharDev(f) => {
                    Self::_istat_helper_chr_file(&f, statbuf);
                }
                Inode::Socket(f) => {
                    Self::_istat_helper_sock(&f, statbuf);
                }
                Inode::Dir(f) => {
                    Self::_istat_helper_dir(&f, statbuf);
                }
            }
            0 //stat has succeeded!
        } else {
            syscall_error(Errno::ENOENT, "stat", "path refers to an invalid file")
        }
    }

    fn _istat_helper(inodeobj: &GenericInode, statbuf: &mut StatData) {
        statbuf.st_mode = inodeobj.mode;
        statbuf.st_nlink = inodeobj.linkcount;
        statbuf.st_uid = inodeobj.uid;
        statbuf.st_gid = inodeobj.gid;
        statbuf.st_rdev = 0;
        statbuf.st_size = inodeobj.size;
        statbuf.st_blksize = 0;
        statbuf.st_blocks = 0;
    }

    fn _istat_helper_sock(inodeobj: &SocketInode, statbuf: &mut StatData) {
        statbuf.st_mode = inodeobj.mode;
        statbuf.st_nlink = inodeobj.linkcount;
        statbuf.st_uid = inodeobj.uid;
        statbuf.st_gid = inodeobj.gid;
        statbuf.st_rdev = 0;
        statbuf.st_size = inodeobj.size;
        statbuf.st_blksize = 0;
        statbuf.st_blocks = 0;
    }

    fn _istat_helper_dir(inodeobj: &DirectoryInode, statbuf: &mut StatData) {
        statbuf.st_mode = inodeobj.mode;
        statbuf.st_nlink = inodeobj.linkcount;
        statbuf.st_uid = inodeobj.uid;
        statbuf.st_gid = inodeobj.gid;
        statbuf.st_rdev = 0;
        statbuf.st_size = inodeobj.size;
        statbuf.st_blksize = 0;
        statbuf.st_blocks = 0;
    }

    fn _istat_helper_chr_file(inodeobj: &DeviceInode, statbuf: &mut StatData) {
        statbuf.st_dev = 5;
        statbuf.st_mode = inodeobj.mode;
        statbuf.st_nlink = inodeobj.linkcount;
        statbuf.st_uid = inodeobj.uid;
        statbuf.st_gid = inodeobj.gid;
        //compose device number into u64
        statbuf.st_rdev = makedev(&inodeobj.dev);
        statbuf.st_size = inodeobj.size;
    }

    //Streams and pipes don't have associated inodes so we populate them from mostly dummy information
    fn _stat_alt_helper(&self, statbuf: &mut StatData, inodenum: usize) {
        statbuf.st_dev = FS_METADATA.dev_id;
        statbuf.st_ino = inodenum;
        statbuf.st_mode = 49590; //r and w priveliged
        statbuf.st_nlink = 1;
        statbuf.st_uid = DEFAULT_UID;
        statbuf.st_gid = DEFAULT_GID;
        statbuf.st_rdev = 0;
        statbuf.st_size = 0;
        statbuf.st_blksize = 0;
        statbuf.st_blocks = 0;
    }

    //------------------------------------FSTAT SYSCALL------------------------------------

    pub fn fstat_syscall(&self, fd: i32, statbuf: &mut StatData) -> i32 {
        let checkedfd = self.get_filedescriptor(fd).unwrap();
        let unlocked_fd = checkedfd.read();
        if let Some(filedesc_enum) = &*unlocked_fd {
            //Delegate populating statbuf to the relevant helper depending on the file type.
            //First we check in the file descriptor to handle sockets, streams, and pipes,
            //and if it is a normal file descriptor we handle regular files, dirs, and char
            //files based on the information in the inode.
            match filedesc_enum {
                File(normalfile_filedesc_obj) => {
                    let inode = FS_METADATA
                        .inodetable
                        .get(&normalfile_filedesc_obj.inode)
                        .unwrap();

                    //populate those fields in statbuf which depend on things other than the inode object
                    statbuf.st_ino = normalfile_filedesc_obj.inode;
                    statbuf.st_dev = FS_METADATA.dev_id;

                    match &*inode {
                        Inode::File(f) => {
                            Self::_istat_helper(&f, statbuf);
                        }
                        Inode::CharDev(f) => {
                            Self::_istat_helper_chr_file(&f, statbuf);
                        }
                        Inode::Socket(f) => {
                            Self::_istat_helper_sock(&f, statbuf);
                        }
                        Inode::Dir(f) => {
                            Self::_istat_helper_dir(&f, statbuf);
                        }
                    }
                }
                Socket(_) => {
                    return syscall_error(
                        Errno::EOPNOTSUPP,
                        "fstat",
                        "we don't support fstat on sockets yet",
                    );
                }
                Stream(_) => {
                    self._stat_alt_helper(statbuf, STREAMINODE);
                }
                Pipe(_) => {
                    self._stat_alt_helper(statbuf, 0xfeef0000);
                }
                Epoll(_) => {
                    self._stat_alt_helper(statbuf, 0xfeef0000);
                }
            }
            0 //fstat has succeeded!
        } else {
            syscall_error(Errno::EBADF, "fstat", "invalid file descriptor")
        }
    }

    //------------------------------------STATFS SYSCALL------------------------------------

    pub fn statfs_syscall(&self, path: &str, databuf: &mut FSData) -> i32 {
        let truepath = normpath(convpath(path), self);

        //Walk the file tree to get inode from path
        if let Some(inodenum) = metawalk(truepath.as_path()) {
            let _inodeobj = FS_METADATA.inodetable.get(&inodenum).unwrap();

            //populate the dev id field -- can be done outside of the helper
            databuf.f_fsid = FS_METADATA.dev_id;

            //delegate the rest of populating statbuf to the relevant helper
            return Self::_istatfs_helper(self, databuf);
        } else {
            syscall_error(Errno::ENOENT, "stat", "path refers to an invalid file")
        }
    }

    //------------------------------------FSTATFS SYSCALL------------------------------------

    pub fn fstatfs_syscall(&self, fd: i32, databuf: &mut FSData) -> i32 {
        let checkedfd = self.get_filedescriptor(fd).unwrap();
        let unlocked_fd = checkedfd.read();
        if let Some(filedesc_enum) = &*unlocked_fd {
            //populate the dev id field -- can be done outside of the helper
            databuf.f_fsid = FS_METADATA.dev_id;

            match filedesc_enum {
                File(normalfile_filedesc_obj) => {
                    let _inodeobj = FS_METADATA
                        .inodetable
                        .get(&normalfile_filedesc_obj.inode)
                        .unwrap();

                    return Self::_istatfs_helper(self, databuf);
                }
                Socket(_) | Pipe(_) | Stream(_) | Epoll(_) => {
                    return syscall_error(
                        Errno::EBADF,
                        "fstatfs",
                        "can't fstatfs on socket, stream, pipe, or epollfd",
                    );
                }
            }
        }
        return syscall_error(Errno::EBADF, "statfs", "invalid file descriptor");
    }

    pub fn _istatfs_helper(&self, databuf: &mut FSData) -> i32 {
        databuf.f_type = 0xBEEFC0DE; //unassigned
        databuf.f_bsize = 4096;
        databuf.f_blocks = 0; //int(limits['diskused']) / 4096
        databuf.f_bfree = 1024 * 1024 * 1024; //(int(limits['diskused']-usage['diskused'])) / 4096
        databuf.f_bavail = 1024 * 1024 * 1024; //(int(limits['diskused']-usage['diskused'])) / 4096
        databuf.f_files = 1024 * 1024 * 1024;
        databuf.f_ffiles = 1024 * 1024 * 515;
        databuf.f_namelen = 254;
        databuf.f_frsize = 4096;
        databuf.f_spare = [0; 32];

        0 //success!
    }

    //------------------------------------READ SYSCALL------------------------------------

    pub fn read_syscall(&self, fd: i32, buf: *mut u8, count: usize) -> i32 {
        let checkedfd = self.get_filedescriptor(fd).unwrap();
        let mut unlocked_fd = checkedfd.write();
        if let Some(filedesc_enum) = &mut *unlocked_fd {
            //delegate to pipe, stream, or socket helper if specified by file descriptor enum type (none of them are implemented yet)
            match filedesc_enum {
                //we must borrow the filedesc object as a mutable reference to update the position
                File(ref mut normalfile_filedesc_obj) => {
                    if is_wronly(normalfile_filedesc_obj.flags) {
                        return syscall_error(
                            Errno::EBADF,
                            "read",
                            "specified file not open for reading",
                        );
                    }

                    let inodeobj = FS_METADATA
                        .inodetable
                        .get(&normalfile_filedesc_obj.inode)
                        .unwrap();

                    //delegate to character if it's a character file, checking based on the type of the inode object
                    match &*inodeobj {
                        Inode::File(_) => {
                            let position = normalfile_filedesc_obj.position;
                            let fileobject =
                                FILEOBJECTTABLE.get(&normalfile_filedesc_obj.inode).unwrap();

                            if let Ok(bytesread) = fileobject.readat(buf, count, position) {
                                //move position forward by the number of bytes we've read

                                normalfile_filedesc_obj.position += bytesread;
                                bytesread as i32
                            } else {
                                0 //0 bytes read, but not an error value that can/should be passed to the user
                            }
                        }

                        Inode::CharDev(char_inode_obj) => {
                            self._read_chr_file(&char_inode_obj, buf, count)
                        }

                        Inode::Socket(_) => {
                            panic!("read(): Socket inode found on a filedesc fd.")
                        }

                        Inode::Dir(_) => syscall_error(
                            Errno::EISDIR,
                            "read",
                            "attempted to read from a directory",
                        ),
                    }
                }
                Socket(_) => {
                    drop(unlocked_fd);
                    self.recv_common(fd, buf, count, 0, &mut None)
                }
                Stream(_) => syscall_error(
                    Errno::EOPNOTSUPP,
                    "read",
                    "reading from stdin not implemented yet",
                ),
                Pipe(pipe_filedesc_obj) => {
                    if is_wronly(pipe_filedesc_obj.flags) {
                        return syscall_error(
                            Errno::EBADF,
                            "read",
                            "specified file not open for reading",
                        );
                    }
                    let mut nonblocking = false;
                    if pipe_filedesc_obj.flags & O_NONBLOCK != 0 {
                        nonblocking = true;
                    }
                    loop {
                        // loop over pipe reads so we can periodically check for cancellation
                        let ret = pipe_filedesc_obj
                            .pipe
                            .read_from_pipe(buf, count, nonblocking)
                            as i32;
                        if pipe_filedesc_obj.flags & O_NONBLOCK == 0
                            && ret == -(Errno::EAGAIN as i32)
                        {
                            if self
                                .cancelstatus
                                .load(interface::RustAtomicOrdering::Relaxed)
                            {
                                // if the cancel status is set in the cage, we trap around a cancel point
                                // until the individual thread is signaled to cancel itself
                                loop {
                                    interface::cancelpoint(self.cageid);
                                }
                            }
                            continue; //received EAGAIN on blocking pipe, try again
                        }
                        return ret; // if we get here we can return
                    }
                }
                Epoll(_) => syscall_error(
                    Errno::EINVAL,
                    "read",
                    "fd is attached to an object which is unsuitable for reading",
                ),
            }
        } else {
            syscall_error(Errno::EBADF, "read", "invalid file descriptor")
        }
    }

    //------------------------------------PREAD SYSCALL------------------------------------
    pub fn pread_syscall(&self, fd: i32, buf: *mut u8, count: usize, offset: isize) -> i32 {
        let checkedfd = self.get_filedescriptor(fd).unwrap();
        let mut unlocked_fd = checkedfd.write();
        if let Some(filedesc_enum) = &mut *unlocked_fd {
            match filedesc_enum {
                //we must borrow the filedesc object as a mutable reference to update the position
                File(ref mut normalfile_filedesc_obj) => {
                    if is_wronly(normalfile_filedesc_obj.flags) {
                        return syscall_error(
                            Errno::EBADF,
                            "pread",
                            "specified file not open for reading",
                        );
                    }

                    let inodeobj = FS_METADATA
                        .inodetable
                        .get(&normalfile_filedesc_obj.inode)
                        .unwrap();

                    //delegate to character if it's a character file, checking based on the type of the inode object
                    match &*inodeobj {
                        Inode::File(_) => {
                            let fileobject =
                                FILEOBJECTTABLE.get(&normalfile_filedesc_obj.inode).unwrap();

                            if let Ok(bytesread) = fileobject.readat(buf, count, offset as usize) {
                                bytesread as i32
                            } else {
                                0 //0 bytes read, but not an error value that can/should be passed to the user
                            }
                        }

                        Inode::CharDev(char_inode_obj) => {
                            self._read_chr_file(&char_inode_obj, buf, count)
                        }

                        Inode::Socket(_) => {
                            panic!("pread(): Socket inode found on a filedesc fd")
                        }

                        Inode::Dir(_) => syscall_error(
                            Errno::EISDIR,
                            "pread",
                            "attempted to read from a directory",
                        ),
                    }
                }
                Socket(_) => syscall_error(
                    Errno::ESPIPE,
                    "pread",
                    "file descriptor is associated with a socket, cannot seek",
                ),
                Stream(_) => syscall_error(
                    Errno::ESPIPE,
                    "pread",
                    "file descriptor is associated with a stream, cannot seek",
                ),
                Pipe(_) => syscall_error(
                    Errno::ESPIPE,
                    "pread",
                    "file descriptor is associated with a pipe, cannot seek",
                ),
                Epoll(_) => syscall_error(
                    Errno::ESPIPE,
                    "pread",
                    "file descriptor is associated with an epollfd, cannot seek",
                ),
            }
        } else {
            syscall_error(Errno::EBADF, "pread", "invalid file descriptor")
        }
    }

    fn _read_chr_file(&self, inodeobj: &DeviceInode, buf: *mut u8, count: usize) -> i32 {
        match inodeobj.dev {
            NULLDEVNO => 0, //reading from /dev/null always reads 0 bytes
            ZERODEVNO => interface::fillzero(buf, count),
            RANDOMDEVNO => interface::fillrandom(buf, count),
            URANDOMDEVNO => interface::fillrandom(buf, count),
            _ => syscall_error(
                Errno::EOPNOTSUPP,
                "read or pread",
                "read from specified device not implemented",
            ),
        }
    }

    //------------------------------------WRITE SYSCALL------------------------------------

    pub fn write_syscall(&self, fd: i32, buf: *const u8, count: usize) -> i32 {
        let checkedfd = self.get_filedescriptor(fd).unwrap();
        let mut unlocked_fd = checkedfd.write();
        if let Some(filedesc_enum) = &mut *unlocked_fd {
            //delegate to pipe, stream, or socket helper if specified by file descriptor enum type
            match filedesc_enum {
                //we must borrow the filedesc object as a mutable reference to update the position
                File(ref mut normalfile_filedesc_obj) => {
                    if is_rdonly(normalfile_filedesc_obj.flags) {
                        return syscall_error(
                            Errno::EBADF,
                            "write",
                            "specified file not open for writing",
                        );
                    }

                    let mut inodeobj = FS_METADATA
                        .inodetable
                        .get_mut(&normalfile_filedesc_obj.inode)
                        .unwrap();

                    //delegate to character helper or print out if it's a character file or stream,
                    //checking based on the type of the inode object
                    match *inodeobj {
                        Inode::File(ref mut normalfile_inode_obj) => {
                            let position = normalfile_filedesc_obj.position;

                            let filesize = normalfile_inode_obj.size;
                            let blankbytecount = position as isize - filesize as isize;

                            let mut fileobject = FILEOBJECTTABLE
                                .get_mut(&normalfile_filedesc_obj.inode)
                                .unwrap();

                            //we need to pad the file with blank bytes if we are at a position past the end of the file!
                            if blankbytecount > 0 {
                                if let Ok(byteswritten) =
                                    fileobject.zerofill_at(filesize, blankbytecount as usize)
                                {
                                    if byteswritten != blankbytecount as usize {
                                        panic!("Write of blank bytes for write failed!");
                                    }
                                } else {
                                    panic!("Write of blank bytes for write failed!");
                                }
                            }

                            let newposition;
                            if let Ok(byteswritten) = fileobject.writeat(buf, count, position) {
                                //move position forward by the number of bytes we've written
                                normalfile_filedesc_obj.position = position + byteswritten;
                                newposition = normalfile_filedesc_obj.position;
                                if newposition > normalfile_inode_obj.size {
                                    normalfile_inode_obj.size = newposition;
                                    drop(inodeobj);
                                    drop(fileobject);
                                    log_metadata(&FS_METADATA, normalfile_filedesc_obj.inode);
                                } //update file size if necessary

                                byteswritten as i32
                            } else {
                                0 //0 bytes written, but not an error value that can/should be passed to the user
                            }
                        }

                        Inode::CharDev(ref char_inode_obj) => {
                            self._write_chr_file(&char_inode_obj, buf, count)
                        }

                        Inode::Socket(_) => {
                            panic!("write(): Socket inode found on a filedesc fd")
                        }

                        Inode::Dir(_) => syscall_error(
                            Errno::EISDIR,
                            "write",
                            "attempted to write to a directory",
                        ),
                    }
                }
                Socket(_) => {
                    drop(unlocked_fd);
                    self.send_syscall(fd, buf, count, 0)
                }
                Stream(stream_filedesc_obj) => {
                    //if it's stdout or stderr, print out and we're done
                    if stream_filedesc_obj.stream == 1 || stream_filedesc_obj.stream == 2 {
                        interface::log_from_ptr(buf, count);
                        count as i32
                    } else {
                        return syscall_error(
                            Errno::EBADF,
                            "write",
                            "specified stream not open for writing",
                        );
                    }
                }
                Pipe(pipe_filedesc_obj) => {
                    if is_rdonly(pipe_filedesc_obj.flags) {
                        return syscall_error(
                            Errno::EBADF,
                            "write",
                            "specified pipe not open for writing",
                        );
                    }

                    let mut nonblocking = false;
                    if pipe_filedesc_obj.flags & O_NONBLOCK != 0 {
                        nonblocking = true;
                    }

                    let retval = pipe_filedesc_obj
                        .pipe
                        .write_to_pipe(buf, count, nonblocking)
                        as i32;
                    if retval == -(Errno::EPIPE as i32) {
                        interface::lind_kill_from_id(self.cageid, SIGPIPE);
                    } // Trigger SIGPIPE
                    retval
                }
                Epoll(_) => syscall_error(
                    Errno::EINVAL,
                    "write",
                    "fd is attached to an object which is unsuitable for writing",
                ),
            }
        } else {
            syscall_error(Errno::EBADF, "write", "invalid file descriptor")
        }
    }

    //------------------------------------PWRITE SYSCALL------------------------------------

    pub fn pwrite_syscall(&self, fd: i32, buf: *const u8, count: usize, offset: isize) -> i32 {
        let checkedfd = self.get_filedescriptor(fd).unwrap();
        let mut unlocked_fd = checkedfd.write();
        if let Some(filedesc_enum) = &mut *unlocked_fd {
            match filedesc_enum {
                //we must borrow the filedesc object as a mutable reference to update the position
                File(ref mut normalfile_filedesc_obj) => {
                    if is_rdonly(normalfile_filedesc_obj.flags) {
                        return syscall_error(
                            Errno::EBADF,
                            "pwrite",
                            "specified file not open for writing",
                        );
                    }

                    let mut inodeobj = FS_METADATA
                        .inodetable
                        .get_mut(&normalfile_filedesc_obj.inode)
                        .unwrap();

                    //delegate to character helper or print out if it's a character file or stream,
                    //checking based on the type of the inode object
                    match *inodeobj {
                        Inode::File(ref mut normalfile_inode_obj) => {
                            let position = offset as usize;
                            let filesize = normalfile_inode_obj.size;
                            let blankbytecount = offset - filesize as isize;

                            let mut fileobject = FILEOBJECTTABLE
                                .get_mut(&normalfile_filedesc_obj.inode)
                                .unwrap();

                            //we need to pad the file with blank bytes if we are seeking past the end of the file!
                            if blankbytecount > 0 {
                                if let Ok(byteswritten) =
                                    fileobject.zerofill_at(filesize, blankbytecount as usize)
                                {
                                    if byteswritten != blankbytecount as usize {
                                        panic!("Write of blank bytes for pwrite failed!");
                                    }
                                } else {
                                    panic!("Write of blank bytes for pwrite failed!");
                                }
                            }

                            let newposition;
                            let retval = if let Ok(byteswritten) =
                                fileobject.writeat(buf, count, position)
                            {
                                //move position forward by the number of bytes we've written
                                newposition = position + byteswritten;

                                byteswritten as i32
                            } else {
                                newposition = position;
                                0 //0 bytes written, but not an error value that can/should be passed to the user
                                  //we still may need to update file size from blank bytes write, so we don't bail out
                            };

                            if newposition > filesize {
                                normalfile_inode_obj.size = newposition;
                                drop(fileobject);
                                drop(inodeobj);
                                log_metadata(&FS_METADATA, normalfile_filedesc_obj.inode);
                            } //update file size if necessary

                            retval
                        }

                        Inode::CharDev(ref char_inode_obj) => {
                            self._write_chr_file(&char_inode_obj, buf, count)
                        }

                        Inode::Socket(_) => {
                            panic!("pwrite: socket fd and inode don't match types")
                        }

                        Inode::Dir(_) => syscall_error(
                            Errno::EISDIR,
                            "pwrite",
                            "attempted to write to a directory",
                        ),
                    }
                }
                Socket(_) => syscall_error(
                    Errno::ESPIPE,
                    "pwrite",
                    "file descriptor is associated with a socket, cannot seek",
                ),
                Stream(_) => syscall_error(
                    Errno::ESPIPE,
                    "pwrite",
                    "file descriptor is associated with a stream, cannot seek",
                ),
                Pipe(_) => syscall_error(
                    Errno::ESPIPE,
                    "pwrite",
                    "file descriptor is associated with a pipe, cannot seek",
                ),
                Epoll(_) => syscall_error(
                    Errno::ESPIPE,
                    "pwrite",
                    "file descriptor is associated with an epollfd, cannot seek",
                ),
            }
        } else {
            syscall_error(Errno::EBADF, "pwrite", "invalid file descriptor")
        }
    }

    fn _write_chr_file(&self, inodeobj: &DeviceInode, _buf: *const u8, count: usize) -> i32 {
        //writes to any of these device files transparently succeed while doing nothing
        match inodeobj.dev {
            NULLDEVNO => count as i32,
            ZERODEVNO => count as i32,
            RANDOMDEVNO => count as i32,
            URANDOMDEVNO => count as i32,
            _ => syscall_error(
                Errno::EOPNOTSUPP,
                "write or pwrite",
                "write to specified device not implemented",
            ),
        }
    }

    //------------------------------------WRITEV SYSCALL------------------------------------

    pub fn writev_syscall(
        &self,
        fd: i32,
        iovec: *const interface::IovecStruct,
        iovcnt: i32,
    ) -> i32 {
        let checkedfd = self.get_filedescriptor(fd).unwrap();
        let mut unlocked_fd = checkedfd.write();
        if let Some(filedesc_enum) = &mut *unlocked_fd {
            // we're only implementing this for INET/tcp sockets right now
            match filedesc_enum {
                Socket(socket_filedesc_obj) => {
                    let sock_tmp = socket_filedesc_obj.handle.clone();
                    let sockhandle = sock_tmp.write();

                    match sockhandle.domain {
                        AF_INET | AF_INET6 => match sockhandle.protocol {
                            IPPROTO_TCP => {
                                // to be able to send here we either need to be fully connected, or connected for write only
                                if (sockhandle.state != ConnState::CONNECTED)
                                    && (sockhandle.state != ConnState::CONNWRONLY)
                                {
                                    return syscall_error(
                                        Errno::ENOTCONN,
                                        "writev",
                                        "The descriptor is not connected",
                                    );
                                }

                                //because socket must be connected it must have an inner raw socket
                                // lets call the kernel writev on that socket
                                let retval = sockhandle
                                    .innersocket
                                    .as_ref()
                                    .unwrap()
                                    .writev(iovec, iovcnt);
                                if retval < 0 {
                                    match Errno::from_discriminant(interface::get_errno()) {
                                        Ok(i) => {
                                            return syscall_error(
                                                i,
                                                "writev",
                                                "The libc call to writev failed!",
                                            );
                                        }
                                        Err(()) => panic!(
                                            "Unknown errno value from socket writev returned!"
                                        ),
                                    };
                                } else {
                                    return retval;
                                }
                            }
                            _ => {
                                return syscall_error(
                                    Errno::EOPNOTSUPP,
                                    "writev",
                                    "System call not implemented for this socket protocol",
                                );
                            }
                        },
                        AF_UNIX => {
                            match sockhandle.protocol {
                                IPPROTO_TCP => {
                                    // to be able to send here we either need to be fully connected, or connected for write only
                                    if (sockhandle.state != ConnState::CONNECTED)
                                        && (sockhandle.state != ConnState::CONNWRONLY)
                                    {
                                        return syscall_error(
                                            Errno::ENOTCONN,
                                            "writev",
                                            "The descriptor is not connected",
                                        );
                                    }
                                    // get the socket pipe, write to it, and return bytes written
                                    let sockinfo = &sockhandle.unix_info.as_ref().unwrap();
                                    let mut nonblocking = false;
                                    if socket_filedesc_obj.flags & O_NONBLOCK != 0 {
                                        nonblocking = true;
                                    }
                                    let retval = match sockinfo.sendpipe.as_ref() {
                                        Some(sendpipe) => sendpipe.write_vectored_to_pipe(
                                            iovec,
                                            iovcnt,
                                            nonblocking,
                                        )
                                            as i32,
                                        None => {
                                            return syscall_error(Errno::EAGAIN, "writev", "there is no data available right now, try again later");
                                        }
                                    };
                                    if retval == -(Errno::EPIPE as i32) {
                                        interface::lind_kill_from_id(self.cageid, SIGPIPE);
                                    } // Trigger SIGPIPE
                                    retval
                                }
                                _ => {
                                    return syscall_error(
                                        Errno::EOPNOTSUPP,
                                        "send",
                                        "Unkown protocol in send",
                                    );
                                }
                            }
                        }
                        _ => {
                            return syscall_error(
                                Errno::EOPNOTSUPP,
                                "writev",
                                "System call not implemented for this socket domain",
                            );
                        }
                    }
                }
                Pipe(pipe_filedesc_obj) => {
                    if is_rdonly(pipe_filedesc_obj.flags) {
                        return syscall_error(
                            Errno::EBADF,
                            "write",
                            "specified pipe not open for writing",
                        );
                    }

                    let mut nonblocking = false;
                    if pipe_filedesc_obj.flags & O_NONBLOCK != 0 {
                        nonblocking = true;
                    }

                    let retval =
                        pipe_filedesc_obj
                            .pipe
                            .write_vectored_to_pipe(iovec, iovcnt, nonblocking)
                            as i32;
                    if retval == -(Errno::EPIPE as i32) {
                        interface::lind_kill_from_id(self.cageid, SIGPIPE);
                    } // Trigger SIGPIPE
                    retval
                }
                _ => {
                    // we currently don't support writev for files/streams
                    return syscall_error(
                        Errno::EOPNOTSUPP,
                        "writev",
                        "System call not implemented for this fd type",
                    );
                }
            }
        } else {
            syscall_error(Errno::EBADF, "write", "invalid file descriptor")
        }
    }

    //------------------------------------LSEEK SYSCALL------------------------------------
    pub fn lseek_syscall(&self, fd: i32, offset: isize, whence: i32) -> i32 {
        let checkedfd = self.get_filedescriptor(fd).unwrap();
        let mut unlocked_fd = checkedfd.write();
        if let Some(filedesc_enum) = &mut *unlocked_fd {
            //confirm fd type is seekable
            match filedesc_enum {
                File(ref mut normalfile_filedesc_obj) => {
                    let inodeobj = FS_METADATA
                        .inodetable
                        .get(&normalfile_filedesc_obj.inode)
                        .unwrap();

                    //handle files/directories differently
                    match &*inodeobj {
                        Inode::File(normalfile_inode_obj) => {
                            let eventualpos = match whence {
                                SEEK_SET => offset,
                                SEEK_CUR => normalfile_filedesc_obj.position as isize + offset,
                                SEEK_END => normalfile_inode_obj.size as isize + offset,
                                _ => {
                                    return syscall_error(Errno::EINVAL, "lseek", "unknown whence");
                                }
                            };

                            if eventualpos < 0 {
                                return syscall_error(
                                    Errno::EINVAL,
                                    "lseek",
                                    "seek to before position 0 in file",
                                );
                            }
                            //subsequent writes to the end of the file must zero pad up until this point if we
                            //overran the end of our file when seeking

                            normalfile_filedesc_obj.position = eventualpos as usize;
                            //return the location that we sought to
                            eventualpos as i32
                        }

                        Inode::CharDev(_) => {
                            0 //for character files, rather than seeking, we transparently do nothing
                        }

                        Inode::Socket(_) => {
                            panic!("lseek: socket fd and inode don't match types")
                        }

                        Inode::Dir(dir_inode_obj) => {
                            //for directories we seek between entries, and thus our end position is the total number of entries
                            let eventualpos = match whence {
                                SEEK_SET => offset,
                                SEEK_CUR => normalfile_filedesc_obj.position as isize + offset,
                                SEEK_END => {
                                    dir_inode_obj.filename_to_inode_dict.len() as isize + offset
                                }
                                _ => {
                                    return syscall_error(Errno::EINVAL, "lseek", "unknown whence");
                                }
                            };

                            //confirm that the location we want to seek to is valid
                            if eventualpos < 0 {
                                return syscall_error(
                                    Errno::EINVAL,
                                    "lseek",
                                    "seek to before position 0 in directory",
                                );
                            }
                            if eventualpos > dir_inode_obj.filename_to_inode_dict.len() as isize {
                                return syscall_error(
                                    Errno::EINVAL,
                                    "lseek",
                                    "seek to after last position in directory",
                                );
                            }

                            normalfile_filedesc_obj.position = eventualpos as usize;
                            //return the location that we sought to
                            eventualpos as i32
                        }
                    }
                }
                Socket(_) => syscall_error(
                    Errno::ESPIPE,
                    "lseek",
                    "file descriptor is associated with a socket, cannot seek",
                ),
                Stream(_) => syscall_error(
                    Errno::ESPIPE,
                    "lseek",
                    "file descriptor is associated with a stream, cannot seek",
                ),
                Pipe(_) => syscall_error(
                    Errno::ESPIPE,
                    "lseek",
                    "file descriptor is associated with a pipe, cannot seek",
                ),
                Epoll(_) => syscall_error(
                    Errno::ESPIPE,
                    "lseek",
                    "file descriptor is associated with an epollfd, cannot seek",
                ),
            }
        } else {
            syscall_error(Errno::EBADF, "lseek", "invalid file descriptor")
        }
    }

    //------------------------------------ACCESS SYSCALL------------------------------------

    pub fn access_syscall(&self, path: &str, amode: u32) -> i32 {
        let truepath = normpath(convpath(path), self);

        //Walk the file tree to get inode from path
        if let Some(inodenum) = metawalk(truepath.as_path()) {
            let inodeobj = FS_METADATA.inodetable.get(&inodenum).unwrap();

            //Get the mode bits if the type of the inode is sane
            let mode = match &*inodeobj {
                Inode::File(f) => f.mode,
                Inode::CharDev(f) => f.mode,
                Inode::Socket(f) => f.mode,
                Inode::Dir(f) => f.mode,
            };

            //We assume that the current user owns the file

            //Construct desired access bits (i.e. 0777) based on the amode parameter
            let mut newmode: u32 = 0;
            if amode & X_OK == X_OK {
                newmode |= S_IXUSR;
            }
            if amode & W_OK == W_OK {
                newmode |= S_IWUSR;
            }
            if amode & R_OK == R_OK {
                newmode |= S_IRUSR;
            }

            //if the desired access bits are compatible with the actual access bits
            //of the file, return a success result, else return a failure result
            if mode & newmode == newmode {
                0
            } else {
                syscall_error(
                    Errno::EACCES,
                    "access",
                    "the requested access would be denied to the file",
                )
            }
        } else {
            syscall_error(
                Errno::ENOENT,
                "access",
                "path does not refer to an existing file",
            )
        }
    }

    //------------------------------------FCHDIR SYSCALL------------------------------------

    pub fn fchdir_syscall(&self, fd: i32) -> i32 {
        let checkedfd = self.get_filedescriptor(fd).unwrap();
        let unlocked_fd = checkedfd.read();

        let path_string = match &*unlocked_fd {
            Some(File(normalfile_filedesc_obj)) => {
                let inodenum = normalfile_filedesc_obj.inode;
                match pathnamefrominodenum(inodenum) {
                    Some(name) => name,
                    None => {
                        return syscall_error(
                            Errno::ENOTDIR,
                            "fchdir",
                            "the file descriptor does not refer to a directory",
                        )
                    }
                }
            }
            Some(_) => {
                return syscall_error(
                    Errno::EACCES,
                    "fchdir",
                    "cannot change working directory on this file descriptor",
                )
            }
            None => return syscall_error(Errno::EBADF, "fchdir", "invalid file descriptor"),
        };

        let mut cwd_container = self.cwd.write();

        *cwd_container = interface::RustRfc::new(convpath(path_string.as_str()));

        0 // fchdir success
    }

    //------------------------------------CHDIR SYSCALL------------------------------------

    pub fn chdir_syscall(&self, path: &str) -> i32 {
        let truepath = normpath(convpath(path), self);
        //Walk the file tree to get inode from path
        if let Some(inodenum) = metawalk(&truepath) {
            if let Inode::Dir(ref mut dir) = *(FS_METADATA.inodetable.get_mut(&inodenum).unwrap()) {
                //increment refcount of new cwd inode to ensure that you can't remove a directory while it is the cwd of a cage
                dir.refcount += 1;
            } else {
                return syscall_error(
                    Errno::ENOTDIR,
                    "chdir",
                    "the last component in path is not a directory",
                );
            }
        } else {
            return syscall_error(
                Errno::ENOENT,
                "chdir",
                "the directory referred to in path does not exist",
            );
        }
        //at this point, syscall isn't an error
        let mut cwd_container = self.cwd.write();

        //decrement refcount of previous cwd's inode, to allow it to be removed if no cage has it as cwd
        decref_dir(&*cwd_container);

        *cwd_container = interface::RustRfc::new(truepath);
        0 //chdir has succeeded!;
    }

    //------------------------------------DUP & DUP2 SYSCALLS------------------------------------

    pub fn dup_syscall(&self, fd: i32, start_desc: Option<i32>) -> i32 {
        //if a starting fd was passed, then use that as the starting point, but otherwise, use the designated minimum of STARTINGFD
        let start_fd = match start_desc {
            Some(start_desc) => start_desc,
            None => STARTINGFD,
        };

        if start_fd == fd {
            return start_fd;
        } //if the file descriptors are equal, return the new one

        // get the filedesc_enum
        let checkedfd = self.get_filedescriptor(fd).unwrap();
        let filedesc_enum = checkedfd.write();
        let filedesc_enum = if let Some(f) = &*filedesc_enum {
            f
        } else {
            return syscall_error(Errno::EBADF, "dup", "Invalid old file descriptor.");
        };

        //checking whether the fd exists in the file table
        return Self::_dup2_helper(&self, filedesc_enum, start_fd, false);
    }

    pub fn dup2_syscall(&self, oldfd: i32, newfd: i32) -> i32 {
        //checking if the new fd is out of range
        if newfd >= MAXFD || newfd < 0 {
            return syscall_error(
                Errno::EBADF,
                "dup2",
                "provided file descriptor is out of range",
            );
        }

        if newfd == oldfd {
            return newfd;
        } //if the file descriptors are equal, return the new one

        // get the filedesc_enum
        let checkedfd = self.get_filedescriptor(oldfd).unwrap();
        let filedesc_enum = checkedfd.write();
        let filedesc_enum = if let Some(f) = &*filedesc_enum {
            f
        } else {
            return syscall_error(Errno::EBADF, "dup2", "Invalid old file descriptor.");
        };

        //if the old fd exists, execute the helper, else return error
        return Self::_dup2_helper(&self, filedesc_enum, newfd, true);
    }

    pub fn _dup2_helper(&self, filedesc_enum: &FileDescriptor, newfd: i32, fromdup2: bool) -> i32 {
        let (dupfd, mut dupfdguard) = if fromdup2 {
            let mut fdguard = self.filedescriptortable[newfd as usize].write();
            let closebool = fdguard.is_some();
            drop(fdguard);
            // close the fd in the way of the new fd. mirror the implementation of linux, ignore the potential error of the close here
            if closebool {
                let _close_result = Self::_close_helper_inner(&self, newfd);
            }

            // re-grab clean fd
            fdguard = self.filedescriptortable[newfd as usize].write();
            (newfd, fdguard)
        } else {
            let (newdupfd, guardopt) = self.get_next_fd(Some(newfd));
            if newdupfd < 0 {
                return syscall_error(
                    Errno::ENFILE,
                    "dup2_helper",
                    "no available file descriptor number could be found",
                );
            }
            (newdupfd, guardopt.unwrap())
        };

        let dupfdoption = &mut *dupfdguard;

        match filedesc_enum {
            File(normalfile_filedesc_obj) => {
                let inodenum = normalfile_filedesc_obj.inode;
                let mut inodeobj = FS_METADATA.inodetable.get_mut(&inodenum).unwrap();
                //incrementing the ref count so that when close is executed on the dup'd file
                //the original file does not get a negative ref count
                match *inodeobj {
                    Inode::File(ref mut normalfile_inode_obj) => {
                        normalfile_inode_obj.refcount += 1;
                    }
                    Inode::Dir(ref mut dir_inode_obj) => {
                        dir_inode_obj.refcount += 1;
                    }
                    Inode::CharDev(ref mut chardev_inode_obj) => {
                        chardev_inode_obj.refcount += 1;
                    }
                    Inode::Socket(_) => panic!("dup: fd and inode do not match."),
                }
            }
            Pipe(pipe_filedesc_obj) => {
                pipe_filedesc_obj.pipe.incr_ref(pipe_filedesc_obj.flags);
            }
            Socket(ref socket_filedesc_obj) => {
                //we handle the closing of sockets on drop
                // checking whether this is a domain socket

                let sock_tmp = socket_filedesc_obj.handle.clone();
                let sockhandle = sock_tmp.write();
                let socket_type = sockhandle.domain;
                if socket_type == AF_UNIX {
                    if let Some(sockinfo) = &sockhandle.unix_info {
                        if let Some(sendpipe) = sockinfo.sendpipe.as_ref() {
                            sendpipe.incr_ref(O_WRONLY);
                        }
                        if let Some(receivepipe) = sockinfo.receivepipe.as_ref() {
                            receivepipe.incr_ref(O_RDONLY);
                        }
                    }
                }
            }
            Stream(_normalfile_filedesc_obj) => {
                // no stream refs
            }
            _ => {
                return syscall_error(Errno::EACCES, "dup or dup2", "can't dup the provided file");
            }
        }

        let mut dupd_fd_enum = filedesc_enum.clone(); //clones the arc for sockethandle

        // get and clone fd, wrap and insert into table.
        match dupd_fd_enum {
            // we don't want to pass on the CLOEXEC flag
            File(ref mut normalfile_filedesc_obj) => {
                normalfile_filedesc_obj.flags = normalfile_filedesc_obj.flags & !O_CLOEXEC;
            }
            Pipe(ref mut pipe_filedesc_obj) => {
                pipe_filedesc_obj.flags = pipe_filedesc_obj.flags & !O_CLOEXEC;
            }
            Socket(ref mut socket_filedesc_obj) => {
                // can do this for domainsockets and sockets
                socket_filedesc_obj.flags = socket_filedesc_obj.flags & !O_CLOEXEC;
            }
            Stream(ref mut stream_filedesc_obj) => {
                stream_filedesc_obj.flags = stream_filedesc_obj.flags & !O_CLOEXEC;
            }
            _ => {
                return syscall_error(Errno::EACCES, "dup or dup2", "can't dup the provided file");
            }
        }

        let _insertval = dupfdoption.insert(dupd_fd_enum);

        return dupfd;
    }

    //------------------------------------CLOSE SYSCALL------------------------------------

    pub fn close_syscall(&self, fd: i32) -> i32 {
        //check that the fd is valid
        return Self::_close_helper(self, fd);
    }

    pub fn _close_helper_inner(&self, fd: i32) -> i32 {
        let checkedfd = self.get_filedescriptor(fd).unwrap();
        let mut unlocked_fd = checkedfd.write();
        if let Some(filedesc_enum) = &mut *unlocked_fd {
            //Decide how to proceed depending on the fd type.
            //First we check in the file descriptor to handle sockets (no-op), sockets (clean the socket), and pipes (clean the pipe),
            //and if it is a normal file descriptor we decrement the refcount to reflect
            //one less reference to the file.
            match filedesc_enum {
                //if we are a socket, we dont change disk metadata
                Stream(_) => {}
                Epoll(_) => {} //Epoll closing not implemented yet
                Socket(ref mut socket_filedesc_obj) => {
                    let sock_tmp = socket_filedesc_obj.handle.clone();
                    let mut sockhandle = sock_tmp.write();

                    // we need to do the following if UDS
                    if let Some(ref mut ui) = sockhandle.unix_info {
                        let inodenum = ui.inode;
                        if let Some(sendpipe) = ui.sendpipe.as_ref() {
                            sendpipe.decr_ref(O_WRONLY);
                            // we're closing the last write end, lets set eof
                            if sendpipe.get_write_ref() == 0 {
                                sendpipe.set_eof();
                            }
                            //last reference, lets remove it
                            if (sendpipe.get_write_ref() as u64) + (sendpipe.get_read_ref() as u64)
                                == 0
                            {
                                ui.sendpipe = None;
                            }
                        }
                        if let Some(receivepipe) = ui.receivepipe.as_ref() {
                            receivepipe.decr_ref(O_RDONLY);
                            //last reference, lets remove it
                            if (receivepipe.get_write_ref() as u64)
                                + (receivepipe.get_read_ref() as u64)
                                == 0
                            {
                                ui.receivepipe = None;
                            }
                        }
                        let mut inodeobj = FS_METADATA.inodetable.get_mut(&ui.inode).unwrap();
                        if let Inode::Socket(ref mut sock) = *inodeobj {
                            sock.refcount -= 1;
                            if sock.refcount == 0 {
                                if sock.linkcount == 0 {
                                    drop(inodeobj);
                                    let path = normpath(
                                        convpath(sockhandle.localaddr.unwrap().path()),
                                        self,
                                    );
                                    FS_METADATA.inodetable.remove(&inodenum);
                                    NET_METADATA.domsock_paths.remove(&path);
                                }
                            }
                        }
                    }
                }
                Pipe(ref pipe_filedesc_obj) => {
                    let pipe = &pipe_filedesc_obj.pipe;
                    pipe.decr_ref(pipe_filedesc_obj.flags);

                    if pipe.get_write_ref() == 0
                        && (pipe_filedesc_obj.flags & O_RDWRFLAGS) == O_WRONLY
                    {
                        // we're closing the last write end, lets set eof
                        pipe.set_eof();
                    }
                }
                File(ref normalfile_filedesc_obj) => {
                    let inodenum = normalfile_filedesc_obj.inode;
                    let mut inodeobj = FS_METADATA.inodetable.get_mut(&inodenum).unwrap();

                    match *inodeobj {
                        Inode::File(ref mut normalfile_inode_obj) => {
                            normalfile_inode_obj.refcount -= 1;

                            //if it's not a reg file, then we have nothing to close
                            //Inode::File is a regular file by default
                            if normalfile_inode_obj.refcount == 0 {
                                FILEOBJECTTABLE
                                    .remove(&inodenum)
                                    .unwrap()
                                    .1
                                    .close()
                                    .unwrap();
                                if normalfile_inode_obj.linkcount == 0 {
                                    drop(inodeobj);
                                    //removing the file from the entire filesystem (interface, metadata, and object table)
                                    FS_METADATA.inodetable.remove(&inodenum);
                                    let sysfilename = format!("{}{}", FILEDATAPREFIX, inodenum);
                                    interface::removefile(sysfilename).unwrap();
                                    log_metadata(&FS_METADATA, inodenum);
                                } else {
                                    drop(inodeobj);
                                }
                            }
                        }
                        Inode::Dir(ref mut dir_inode_obj) => {
                            dir_inode_obj.refcount -= 1;

                            //if it's not a reg file, then we have nothing to close
                            match FILEOBJECTTABLE.get(&inodenum) {
                                Some(_) => {
                                    return syscall_error(
                                        Errno::ENOEXEC,
                                        "close or dup",
                                        "Non-regular file in file object table",
                                    );
                                }
                                None => {}
                            }
                            if dir_inode_obj.linkcount == 2 && dir_inode_obj.refcount == 0 {
                                //removing the file from the metadata
                                FS_METADATA.inodetable.remove(&inodenum);
                                drop(inodeobj);
                                log_metadata(&FS_METADATA, inodenum);
                            }
                        }
                        Inode::CharDev(ref mut char_inode_obj) => {
                            char_inode_obj.refcount -= 1;

                            //if it's not a reg file, then we have nothing to close
                            match FILEOBJECTTABLE.get(&inodenum) {
                                Some(_) => {
                                    return syscall_error(
                                        Errno::ENOEXEC,
                                        "close or dup",
                                        "Non-regular file in file object table",
                                    );
                                }
                                None => {}
                            }
                            if char_inode_obj.linkcount == 0 && char_inode_obj.refcount == 0 {
                                //removing the file from the metadata
                                drop(inodeobj);
                                FS_METADATA.inodetable.remove(&inodenum);
                            } else {
                                drop(inodeobj);
                            }
                            log_metadata(&FS_METADATA, inodenum);
                        }
                        Inode::Socket(_) => {
                            panic!("close(): Socket inode found on a filedesc fd.")
                        }
                    }
                }
            }
            0
        } else {
            return syscall_error(Errno::EBADF, "close", "invalid file descriptor");
        }
    }

    pub fn _close_helper(&self, fd: i32) -> i32 {
        let inner_result = self._close_helper_inner(fd);
        if inner_result < 0 {
            return inner_result;
        }

        //removing inode from fd table
        let checkedfd = self.get_filedescriptor(fd).unwrap();
        let mut unlocked_fd = checkedfd.write();
        if unlocked_fd.is_some() {
            let _discarded_fd = unlocked_fd.take();
        }
        0 //_close_helper has succeeded!
    }

    //------------------------------------FCNTL SYSCALL------------------------------------
    
    //fcntl performs operations, like returning or setting file status flags,
    //duplicating a file descriptor, etc., on an open file descriptor 
    //it accepts three parameters: fd - an open file descriptor, cmd - an operation to be performed on fd,
    //and arg - an optional argument (whether or not arg is required is determined by cmd)
    //for a successful call, the return value depends on the operation and can be one of: zero, the new file descriptor, 
    //value of file descriptor flags, value of status flags, etc.
    //for more detailed description of all the commands and return values, see 
    //https://linux.die.net/man/2/fcntl

    pub fn fcntl_syscall(&self, fd: i32, cmd: i32, arg: i32) -> i32 {
        //BUG
        //if the provided file descriptor is out of bounds, get_filedescriptor returns Err(),
        //unwrapping on which  produces a 'panic!'
        //otherwise, file descriptor table entry is stored in 'checkedfd'
        let checkedfd = self.get_filedescriptor(fd).unwrap();
        let mut unlocked_fd = checkedfd.write();
        if let Some(filedesc_enum) = &mut *unlocked_fd {                    
            //'flags' consists of bitwise-or'd access mode, file creation, and file status flags
            //to retrieve a particular flag, it can bitwise-and'd with 'flags'
            let flags = match filedesc_enum {
                Epoll(obj) => &mut obj.flags,
                Pipe(obj) => &mut obj.flags,
                Stream(obj) => &mut obj.flags,
                File(obj) => &mut obj.flags,
                //not clear why running F_SETFL on Socket type requires special treatment
                Socket(ref mut sockfdobj) => {
                    if cmd == F_SETFL && arg >= 0 {
                        let sock_tmp = sockfdobj.handle.clone();
                        let mut sockhandle = sock_tmp.write();

                        if let Some(ins) = &mut sockhandle.innersocket {
                            let fcntlret;
                            if arg & O_NONBLOCK == O_NONBLOCK {
                                //set non-blocking I/O
                                fcntlret = ins.set_nonblocking();
                            } else {
                                //set blocking I/O
                                fcntlret = ins.set_blocking();
                            }
                            if fcntlret < 0 {
                                match Errno::from_discriminant(interface::get_errno()) {
                                    Ok(i) => {
                                        return syscall_error(
                                            i,
                                            "fcntl",
                                            "The libc call to fcntl failed!",
                                        );
                                    }
                                    Err(()) => panic!("Unknown errno value from fcntl returned!"),
                                };
                            }
                        }
                    }

                    &mut sockfdobj.flags
                }
            };

            //matching the tuple
            match (cmd, arg) {
                //because the arg parameter is not used in certain commands, it can be anything (..)
                //F_GETFD returns file descriptor flags only, meaning that access mode flags 
                //and file status flags are excluded
                //F_SETFD is used to set file descriptor flags only, meaning that any changes to access mode flags
                //or file status flags should be ignored
                //currently, O_CLOEXEC is the only defined file descriptor flag, thus only this flag is
                //masked when using F_GETFD or F_SETFD
                (F_GETFD, ..) => *flags & O_CLOEXEC,
                (F_SETFD, arg) if arg >= 0 => {
                    if arg & O_CLOEXEC != 0 {
                        //if O_CLOEXEC flag is set to 1 in 'arg', 'flags' is updated by setting its O_CLOEXEC bit to 1
                        *flags |= O_CLOEXEC;
                    } else {
                        //if O_CLOEXEC flag is set to 0 in 'arg', 'flags' is updated by setting its O_CLOEXEC bit to 0
                        *flags &= !O_CLOEXEC;
                    }
                    0
                }
                //F_GETFL should return file access mode and file status flags, which means that
                //file creation flags should be masked out
                (F_GETFL, ..) => {
                    *flags & !(O_CREAT | O_EXCL | O_NOCTTY | O_TRUNC)
                }
                //F_SETFL is used to set file status flags, thus any changes to file access mode and file
                //creation flags should be ignored (see F_SETFL command in the man page for fcntl for the reference)
                (F_SETFL, arg) if arg >= 0 => {
                    //valid changes are extracted by ignoring changes to file access mode and file creation flags
                    let valid_changes = arg & !(O_RDWRFLAGS | O_CREAT | O_EXCL | O_NOCTTY | O_TRUNC);
                    //access mode and creation flags are extracted and other flags are set to 0 to update them
                    let acc_and_creation_flags = *flags & (O_RDWRFLAGS | O_CREAT | O_EXCL | O_NOCTTY | O_TRUNC);
                    //valid changes are combined with the old file access mode and file creation flags
                    *flags = valid_changes | acc_and_creation_flags; 
                    0
                }
                (F_DUPFD, arg) if arg >= 0 => self._dup2_helper(&filedesc_enum, arg, false),
                //TO DO: F_GETOWN and F_SETOWN commands are not implemented yet
                (F_GETOWN, ..) => {
                    0 
                }
                (F_SETOWN, arg) if arg >= 0 => {
                    0
                }
                _ => {
                    let err_msg = format!("Arguments pair ({}, {}) does not match implemented parameters", cmd, arg);
                    syscall_error(Errno::EINVAL, "fcntl", &err_msg)
                },
            }
        } else {
            syscall_error(Errno::EBADF, "fcntl", "File descriptor is out of range")
        }
    }

    /// ### Description
    ///
    /// The `ioctl_syscall()` manipulates the underlying device parameters of special files. In particular, it is used as a way
    /// for user-space applications to interface with device drivers. 
    ///
    /// ### Arguments
    ///
    /// The `ioctl_syscall()` accepts three arguments:
    /// * `fd` - an open file descriptor that refers to a device.
    /// * `request` - the control function to be performed. The set of valid request values depends entirely on the device
    ///              being addressed. MEDIA_IOC_DEVICE_INFO is an example of an ioctl control function to query device
    ///              information that all media devices must support.
    /// * `ptrunion` - additional information needed by the addressed device to perform the selected control function.
    ///              In the example of MEDIA_IOC_DEVICE_INFO request, a valid ptrunion value is a pointer to a struct 
    ///              media_device_info, from which the device information is obtained.
    ///
    /// ### Returns
    ///
    /// Upon successful completion, a value other than -1 that depends on the selected control function is returned.
    /// In case of a failure, -1 is returned with errno set to a particular value, like EBADF, EINVAL, etc.
    ///
    /// ### Errors and Panics
    ///
    /// * `EBADF` - fd is not a valid file descriptor
    /// * `EFAULT` - ptrunion references an inaccessible memory area
    /// * `EINVAL` - request or ptrunion is not valid
    /// * `ENOTTY` - fd is not associated with a character special device
    /// When `ioctl_syscall() is called on a Socket with `FIONBIO` control function, an underlying call to `libc::fcntl()` is made,
    /// which can return with an error. For a complete list of possible erorrs, see 
    /// [fcntl(2)](https://linux.die.net/man/2/fcntl)
    ///
    /// A panic occurs either when a provided file descriptor is out of bounds or when
    /// an underlying call to `libc::fcntl()` for Socket type is returned with an unknown error.
    ///
    /// To learn more about the syscall, control functions applicable to all the devices, and possible error values, see
    /// [ioctl(2)](https://man.openbsd.org/ioctl)

    pub fn ioctl_syscall(&self, fd: i32, request: u32, ptrunion: IoctlPtrUnion) -> i32 {
        //BUG
        //if the provided file descriptor is out of bounds, 'get_filedescriptor' returns Err(),
        //unwrapping on which  produces a 'panic!'
        //otherwise, file descriptor table entry is stored in 'checkedfd'
        let checkedfd = self.get_filedescriptor(fd).unwrap();
        let mut unlocked_fd = checkedfd.write();
        //if a table descriptor entry is non-empty, a valid request is performed
        if let Some(filedesc_enum) = &mut *unlocked_fd {
            //For now, the only implemented control function is FIONBIO command used with sockets 
            match request {
                //for FIONBIO, 'ptrunion' stores a pointer to an integer. If the integer is 0, the socket's
                //nonblocking I/O is cleared. Otherwise, the socket is set for nonblocking I/O
                FIONBIO => {
                    //if 'ptrunion' stores a Null pointer, a 'Bad address' error is returned
                    //otheriwse, the integer value stored in that address is returned and saved into 'arg_result'
                    let arg_result = interface::get_ioctl_int(ptrunion);
                    match (arg_result, filedesc_enum) {
                        (Err(arg_result), ..)=> {
                            return arg_result;
                        }
                        //since FIONBIO command is used with sockets, we need to make sure that the provided
                        //file descriptor addresses a socket
                        //otherwise, a 'Not a typewriter' error designating that the specified command
                        //is only applicable to sockets is returned 
                        (Ok(arg_result), Socket(ref mut sockfdobj)) => {
                            let sock_tmp = sockfdobj.handle.clone();
                            let mut sockhandle = sock_tmp.write();
                            let flags = &mut sockfdobj.flags;
                            let arg: i32 = arg_result;
                            let mut ioctlret = 0;
                            //clearing nonblocking I/O on the socket if the integer is 0
                            if arg == 0 { 
                                *flags &= !O_NONBLOCK;
                                //libc::fcntl is called under the hood with F_SETFL command and 0 as an argument
                                //to set blocking I/O, and the result of the call is stored in ioctlret
                                if let Some(ins) = &mut sockhandle.innersocket {

                                    ioctlret = ins.set_blocking();
                                }
                            } else {
                                *flags |= O_NONBLOCK;
                                //libc::fcntl is called under the hood with F_SETFL command ans O_NONBLOCK as an argument
                                //to set nonblocking I/O, and the result of the call is stored in ioctlret
                                if let Some(ins) = &mut sockhandle.innersocket {
                                    ioctlret = ins.set_nonblocking();
                                }
                            }
                            //if ioctlret is negative, it means that the call to fcntl returned with an error
                            if ioctlret < 0 {
                                match Errno::from_discriminant(interface::get_errno()) {
                                    Ok(i) => {return syscall_error(i, "ioctl", "The libc call to ioctl failed!");},
                                    Err(()) => panic!("Unknown errno value from ioctl returned!"),
                                };
                            }

                            0
                        }
                        _ => {syscall_error(Errno::ENOTTY, "ioctl", "The specified request does not apply to the kind of object that the file descriptor fd references.")}
                    }
                }
                FIOASYNC => {
                    //not implemented
                    interface::log_verbose(
                        "ioctl(FIOASYNC) is not implemented, and just returns 0.",
                    );
                    0
                }
                _ => syscall_error(
                    Errno::EINVAL,
                    "ioctl",
                    "Arguments provided do not match implemented parameters",
                ),
            }
        } else {
            syscall_error(Errno::EBADF, "ioctl", "Invalid file descriptor")
        }
    }

    //------------------------------------CHMOD HELPER FUNCTION------------------------------------

    pub fn _chmod_helper(inodenum: usize, mode: u32) {
        let mut thisinode = FS_METADATA.inodetable.get_mut(&inodenum).unwrap();
        let mut log = true;
        if mode & (S_IRWXA | (S_FILETYPEFLAGS as u32)) == mode {
            match *thisinode {
                Inode::File(ref mut general_inode) => {
                    general_inode.mode = (general_inode.mode & !S_IRWXA) | mode
                }
                Inode::CharDev(ref mut dev_inode) => {
                    dev_inode.mode = (dev_inode.mode & !S_IRWXA) | mode;
                }
                Inode::Socket(ref mut sock_inode) => {
                    sock_inode.mode = (sock_inode.mode & !S_IRWXA) | mode;
                    log = false;
                }
                Inode::Dir(ref mut dir_inode) => {
                    dir_inode.mode = (dir_inode.mode & !S_IRWXA) | mode;
                }
            }
            drop(thisinode);
            if log {
                log_metadata(&FS_METADATA, inodenum)
            };
        }
    }

    //------------------------------------CHMOD SYSCALL------------------------------------

    pub fn chmod_syscall(&self, path: &str, mode: u32) -> i32 {
        let truepath = normpath(convpath(path), self);

        //check if there is a valid path or not there to an inode
        if let Some(inodenum) = metawalk(truepath.as_path()) {
            if mode & (S_IRWXA | (S_FILETYPEFLAGS as u32)) == mode {
                Self::_chmod_helper(inodenum, mode);
            } else {
                //there doesn't seem to be a good syscall error errno for this
                return syscall_error(Errno::EACCES, "chmod", "provided file mode is not valid");
            }
        } else {
            return syscall_error(Errno::ENOENT, "chmod", "the provided path does not exist");
        }
        0 //success!
    }

    //------------------------------------FCHMOD SYSCALL------------------------------------

    pub fn fchmod_syscall(&self, fd: i32, mode: u32) -> i32 {
        let checkedfd = self.get_filedescriptor(fd).unwrap();
        let unlocked_fd = checkedfd.read();
        if let Some(filedesc_enum) = &*unlocked_fd {
            match filedesc_enum {
                File(normalfile_filedesc_obj) => {
                    let inodenum = normalfile_filedesc_obj.inode;
                    if mode & (S_IRWXA | (S_FILETYPEFLAGS as u32)) == mode {
                        Self::_chmod_helper(inodenum, mode);
                    } else {
                        return syscall_error(
                            Errno::EACCES,
                            "fchmod",
                            "provided file mode is not valid",
                        );
                    }
                }
                Socket(_) => {
                    return syscall_error(
                        Errno::EACCES,
                        "fchmod",
                        "cannot change mode on this file descriptor",
                    );
                }
                Stream(_) => {
                    return syscall_error(
                        Errno::EACCES,
                        "fchmod",
                        "cannot change mode on this file descriptor",
                    );
                }
                Pipe(_) => {
                    return syscall_error(
                        Errno::EACCES,
                        "fchmod",
                        "cannot change mode on this file descriptor",
                    );
                }
                Epoll(_) => {
                    return syscall_error(
                        Errno::EACCES,
                        "fchmod",
                        "cannot change mode on this file descriptor",
                    );
                }
            }
        } else {
            return syscall_error(
                Errno::ENOENT,
                "fchmod",
                "the provided file descriptor  does not exist",
            );
        }
        0 //success!
    }

    //------------------------------------MMAP SYSCALL------------------------------------

    pub fn mmap_syscall(
        &self,
        addr: *mut u8,
        len: usize,
        prot: i32,
        flags: i32,
        fildes: i32,
        off: i64,
    ) -> i32 {
        if len == 0 {
            syscall_error(Errno::EINVAL, "mmap", "the value of len is 0");
        }

        if 0 == flags & (MAP_PRIVATE | MAP_SHARED) {
            syscall_error(
                Errno::EINVAL,
                "mmap",
                "The value of flags is invalid (neither MAP_PRIVATE nor MAP_SHARED is set)",
            );
        }

        if 0 != flags & MAP_ANONYMOUS {
            return interface::libc_mmap(addr, len, prot, flags, -1, 0);
        }

        let checkedfd = self.get_filedescriptor(fildes).unwrap();
        let mut unlocked_fd = checkedfd.write();
        if let Some(filedesc_enum) = &mut *unlocked_fd {
            //confirm fd type is mappable
            match filedesc_enum {
                File(ref mut normalfile_filedesc_obj) => {
                    let inodeobj = FS_METADATA
                        .inodetable
                        .get(&normalfile_filedesc_obj.inode)
                        .unwrap();

                    //confirm inode type is mappable
                    match &*inodeobj {
                        Inode::File(normalfile_inode_obj) => {
                            //if we want to write our changes back to the file the file needs to be open for reading and writing
                            if (flags & MAP_SHARED != 0) && (flags & PROT_WRITE != 0) && (normalfile_filedesc_obj.flags & O_RDWR != 0) {
                                return syscall_error(Errno::EACCES, "mmap", "file descriptor is not open RDWR, but MAP_SHARED and PROT_WRITE are set");
                            }
                            let filesize = normalfile_inode_obj.size;
                            if off < 0 || off > filesize as i64 {
                                return syscall_error(Errno::ENXIO, "mmap", "Addresses in the range [off,off+len) are invalid for the object specified by fildes.");
                            }
                            //because of NaCl's internal workings we must allow mappings to extend past the end of a file
                            let fobj = FILEOBJECTTABLE.get(&normalfile_filedesc_obj.inode).unwrap();
                            //we cannot mmap a rust file in quite the right way so we retrieve the fd number from it
                            //this is the system fd number--the number of the lind.<inodenum> file in our host system
                            let fobjfdno = fobj.as_fd_handle_raw_int();


                            interface::libc_mmap(addr, len, prot, flags, fobjfdno, off)
                        }

                        Inode::CharDev(_chardev_inode_obj) => {
                            syscall_error(Errno::EOPNOTSUPP, "mmap", "lind currently does not support mapping character files")
                        }

                        _ => {syscall_error(Errno::EACCES, "mmap", "the fildes argument refers to a file whose type is not supported by mmap")}
                    }
                }
                _ => syscall_error(
                    Errno::EACCES,
                    "mmap",
                    "the fildes argument refers to a file whose type is not supported by mmap",
                ),
            }
        } else {
            syscall_error(Errno::EBADF, "mmap", "invalid file descriptor")
        }
    }

    //------------------------------------MUNMAP SYSCALL------------------------------------

    pub fn munmap_syscall(&self, addr: *mut u8, len: usize) -> i32 {
        if len == 0 {
            syscall_error(Errno::EINVAL, "mmap", "the value of len is 0");
        }
        //NaCl's munmap implementation actually just writes over the previously mapped data with PROT_NONE
        //This frees all of the resources except page table space, and is put inside safeposix for consistency
        interface::libc_mmap(
            addr,
            len,
            PROT_NONE,
            MAP_PRIVATE | MAP_ANONYMOUS | MAP_FIXED,
            -1,
            0,
        )
    }

    //------------------------------------FLOCK SYSCALL------------------------------------

    pub fn flock_syscall(&self, fd: i32, operation: i32) -> i32 {
        let checkedfd = self.get_filedescriptor(fd).unwrap();
        let unlocked_fd = checkedfd.read();
        if let Some(filedesc_enum) = &*unlocked_fd {
            let lock = match filedesc_enum {
                File(normalfile_filedesc_obj) => &normalfile_filedesc_obj.advlock,
                Socket(socket_filedesc_obj) => &socket_filedesc_obj.advlock,
                Stream(stream_filedesc_obj) => &stream_filedesc_obj.advlock,
                Pipe(pipe_filedesc_obj) => &pipe_filedesc_obj.advlock,
                Epoll(epoll_filedesc_obj) => &epoll_filedesc_obj.advlock,
            };
            match operation & (LOCK_SH | LOCK_EX | LOCK_UN) {
                LOCK_SH => {
                    if operation & LOCK_NB == LOCK_NB {
                        //EAGAIN and EWOULDBLOCK are the same
                        if !lock.try_lock_sh() {
                            return syscall_error(
                                Errno::EAGAIN,
                                "flock",
                                "shared lock would block",
                            );
                        };
                    } else {
                        lock.lock_sh();
                    }
                }
                LOCK_EX => {
                    if operation & LOCK_NB == LOCK_NB {
                        if !lock.try_lock_ex() {
                            return syscall_error(
                                Errno::EAGAIN,
                                "flock",
                                "exclusive lock would block",
                            );
                        };
                    } else {
                        lock.lock_ex();
                    }
                }
                LOCK_UN => {
                    if operation & LOCK_NB == LOCK_NB {
                        lock.unlock();
                    } else {
                        lock.unlock();
                    }
                }
                _ => {
                    return syscall_error(Errno::EINVAL, "flock", "unknown operation");
                }
            }
            0 //flock has  succeeded!
        } else {
            syscall_error(Errno::EBADF, "flock", "invalid file descriptor")
        }
    }

    pub fn remove_from_parent_dir(
        parent_inodenum: usize,
        truepath: &interface::RustPathBuf,
    ) -> i32 {
        if let Inode::Dir(ref mut parent_dir) =
            *(FS_METADATA.inodetable.get_mut(&parent_inodenum).unwrap())
        {
            // check if parent dir has write permission
            if parent_dir.mode as u32 & (S_IWOTH | S_IWGRP | S_IWUSR) == 0 {
                return syscall_error(
                    Errno::EPERM,
                    "rmdir",
                    "Parent directory does not have write permission",
                );
            }

            // remove entry of corresponding filename from filename-inode dict
            parent_dir
                .filename_to_inode_dict
                .remove(&truepath.file_name().unwrap().to_str().unwrap().to_string())
                .unwrap();
            parent_dir.linkcount -= 1; // decrement linkcount of parent dir
        } else {
            panic!("Non directory file was parent!");
        }
        0
    }

    //------------------RMDIR SYSCALL------------------

    pub fn rmdir_syscall(&self, path: &str) -> i32 {
        if path.len() == 0 {
            return syscall_error(Errno::ENOENT, "rmdir", "Given path is null");
        }
        let truepath = normpath(convpath(path), self);

        // try to get inodenum of input path and its parent
        match metawalkandparent(truepath.as_path()) {
            (None, ..) => syscall_error(Errno::ENOENT, "rmdir", "Path does not exist"),
            (Some(_), None) => {
                // path exists but parent does not => path is root dir
                syscall_error(Errno::EBUSY, "rmdir", "Cannot remove root directory")
            }
            (Some(inodenum), Some(parent_inodenum)) => {
                let mut inodeobj = FS_METADATA.inodetable.get_mut(&inodenum).unwrap();

                match &mut *inodeobj {
                    // make sure inode matches a directory
                    Inode::Dir(ref mut dir_obj) => {
                        if dir_obj.linkcount > 3 {
                            return syscall_error(
                                Errno::ENOTEMPTY,
                                "rmdir",
                                "Directory is not empty",
                            );
                        }
                        if !is_dir(dir_obj.mode) {
                            panic!("This directory does not have its mode set to S_IFDIR");
                        }

                        // check if dir has write permission
                        if dir_obj.mode as u32 & (S_IWOTH | S_IWGRP | S_IWUSR) == 0 {
                            return syscall_error(
                                Errno::EPERM,
                                "rmdir",
                                "Directory does not have write permission",
                            );
                        }

                        let remove_inode = dir_obj.refcount == 0;
                        if remove_inode {
                            dir_obj.linkcount = 2;
                        } // linkcount for an empty directory after rmdir must be 2
                        drop(inodeobj);

                        let removal_result =
                            Self::remove_from_parent_dir(parent_inodenum, &truepath);
                        if removal_result != 0 {
                            return removal_result;
                        }

                        // remove entry of corresponding inodenum from inodetable
                        if remove_inode {
                            FS_METADATA.inodetable.remove(&inodenum).unwrap();
                        }

                        log_metadata(&FS_METADATA, parent_inodenum);
                        log_metadata(&FS_METADATA, inodenum);
                        0 // success
                    }
                    _ => syscall_error(Errno::ENOTDIR, "rmdir", "Path is not a directory"),
                }
            }
        }
    }

    //------------------RENAME SYSCALL------------------

    pub fn rename_syscall(&self, oldpath: &str, newpath: &str) -> i32 {
        if oldpath.len() == 0 {
            return syscall_error(Errno::ENOENT, "rename", "Old path is null");
        }
        if newpath.len() == 0 {
            return syscall_error(Errno::ENOENT, "rename", "New path is null");
        }

        let true_oldpath = normpath(convpath(oldpath), self);
        let true_newpath = normpath(convpath(newpath), self);

        // try to get inodenum of old path and its parent
        match metawalkandparent(true_oldpath.as_path()) {
            (None, ..) => syscall_error(Errno::EEXIST, "rename", "Old path does not exist"),
            (Some(_), None) => {
                syscall_error(Errno::EBUSY, "rename", "Cannot rename root directory")
            }
            (Some(inodenum), Some(parent_inodenum)) => {
                // make sure file is not moved to another dir
                // get inodenum for parent of new path
                let (_, new_par_inodenum) = metawalkandparent(true_newpath.as_path());
                // check if old and new paths share parent
                if new_par_inodenum != Some(parent_inodenum) {
                    return syscall_error(
                        Errno::EOPNOTSUPP,
                        "rename",
                        "Cannot move file to another directory",
                    );
                }

                let pardir_inodeobj = FS_METADATA.inodetable.get_mut(&parent_inodenum).unwrap();
                if let Inode::Dir(parent_dir) = &*pardir_inodeobj {
                    // add pair of new path and its inodenum to filename-inode dict
                    parent_dir.filename_to_inode_dict.insert(
                        true_newpath
                            .file_name()
                            .unwrap()
                            .to_str()
                            .unwrap()
                            .to_string(),
                        inodenum,
                    );

                    // remove entry of old path from filename-inode dict
                    parent_dir.filename_to_inode_dict.remove(
                        &true_oldpath
                            .file_name()
                            .unwrap()
                            .to_str()
                            .unwrap()
                            .to_string(),
                    );
                    drop(pardir_inodeobj);
                    log_metadata(&FS_METADATA, parent_inodenum);
                }
                NET_METADATA.domsock_paths.insert(true_newpath);
                NET_METADATA.domsock_paths.remove(&true_oldpath);
                0 // success
            }
        }
    }

    fn _truncate_helper(&self, inodenum: usize, length: isize, file_must_exist: bool) -> i32 {
        if length < 0 {
            return syscall_error(Errno::EINVAL, "truncate", "length specified as less than 0");
        }
        let mut inodeobj = FS_METADATA.inodetable.get_mut(&inodenum).unwrap();

        match *inodeobj {
            // only proceed when inode matches with a file
            Inode::File(ref mut normalfile_inode_obj) => {
                let ulength = length as usize;
                let filesize = normalfile_inode_obj.size as usize;

                // get file object table with write lock
                let mut maybe_fileobject = FILEOBJECTTABLE.entry(inodenum);
                let mut tempbind;
                let close_on_exit;

                //We check if the fileobject exists. If file_must_exist is true (i.e. we called the helper from
                //ftruncate) then we know that an fd must exist and thus we panic if the fileobject does not
                //exist. If file_must_exist is false (i.e. we called the helper from truncate), if the file does
                //not exist,  we create a new fileobject to use which we remove once we are done with it
                let fileobject = if let interface::RustHashEntry::Occupied(ref mut occ) =
                    maybe_fileobject
                {
                    close_on_exit = false;
                    occ.get_mut()
                } else if file_must_exist {
                    panic!("Somehow a normal file with an fd was truncated but there was no file object in rustposix?");
                } else {
                    let sysfilename = format!("{}{}", FILEDATAPREFIX, inodenum);
                    tempbind = interface::openfile(sysfilename, filesize).unwrap(); // open file with size given from inode
                    close_on_exit = true;
                    &mut tempbind
                };

                // if length is greater than original filesize,
                // file is extented with null bytes
                if filesize < ulength {
                    let blankbytecount = ulength - filesize;
                    if let Ok(byteswritten) = fileobject.zerofill_at(filesize, blankbytecount) {
                        if byteswritten != blankbytecount {
                            panic!("zerofill_at() has failed");
                        }
                    } else {
                        panic!("zerofill_at() has failed");
                    }
                } else {
                    // if length is smaller than original filesize,
                    // extra data are cut off
                    fileobject.shrink(ulength).unwrap();
                }

                if close_on_exit {
                    fileobject.close().unwrap();
                }

                drop(maybe_fileobject);

                normalfile_inode_obj.size = ulength;

                drop(inodeobj);
                log_metadata(&FS_METADATA, inodenum);
                0 // truncating has succeeded!
            }
            Inode::CharDev(_) => syscall_error(
                Errno::EINVAL,
                "truncate",
                "The named file is a character driver",
            ),
            Inode::Socket(_) => syscall_error(
                Errno::EINVAL,
                "truncate",
                "The named file is a domain socket",
            ),
            Inode::Dir(_) => {
                syscall_error(Errno::EISDIR, "truncate", "The named file is a directory")
            }
        }
    }

    //------------------------------------FSYNC SYSCALL------------------------------------

    pub fn fsync_syscall(&self, fd: i32) -> i32 {
        let checkedfd = self.get_filedescriptor(fd).unwrap();
        let mut unlocked_fd = checkedfd.write();
        if let Some(filedesc_enum) = &mut *unlocked_fd {
            match filedesc_enum {
                File(ref mut normalfile_filedesc_obj) => {
                    if is_rdonly(normalfile_filedesc_obj.flags) {
                        return syscall_error(
                            Errno::EBADF,
                            "fsync",
                            "specified file not open for sync",
                        );
                    }
                    let inodeobj = FS_METADATA
                        .inodetable
                        .get(&normalfile_filedesc_obj.inode)
                        .unwrap();
                    match &*inodeobj {
                        Inode::File(_) => {
                            let fileobject =
                                FILEOBJECTTABLE.get(&normalfile_filedesc_obj.inode).unwrap();

                            match fileobject.fsync() {
                                Ok(_) => 0,
                                _ => syscall_error(
                                    Errno::EIO,
                                    "fsync",
                                    "an error occurred during synchronization",
                                ),
                            }
                        }
                        _ => syscall_error(
                            Errno::EROFS,
                            "fsync",
                            "does not support special files for synchronization",
                        ),
                    }
                }
                _ => syscall_error(
                    Errno::EINVAL,
                    "fsync",
                    "fd is attached to an object which is unsuitable for synchronization",
                ),
            }
        } else {
            syscall_error(Errno::EBADF, "fsync", "invalid file descriptor")
        }
    }

    //------------------------------------FDATASYNC SYSCALL------------------------------------

    pub fn fdatasync_syscall(&self, fd: i32) -> i32 {
        let checkedfd = self.get_filedescriptor(fd).unwrap();
        let mut unlocked_fd = checkedfd.write();
        if let Some(filedesc_enum) = &mut *unlocked_fd {
            match filedesc_enum {
                File(ref mut normalfile_filedesc_obj) => {
                    if is_rdonly(normalfile_filedesc_obj.flags) {
                        return syscall_error(
                            Errno::EBADF,
                            "fdatasync",
                            "specified file not open for sync",
                        );
                    }
                    let inodeobj = FS_METADATA
                        .inodetable
                        .get(&normalfile_filedesc_obj.inode)
                        .unwrap();
                    match &*inodeobj {
                        Inode::File(_) => {
                            let fileobject =
                                FILEOBJECTTABLE.get(&normalfile_filedesc_obj.inode).unwrap();

                            match fileobject.fdatasync() {
                                Ok(_) => 0,
                                _ => syscall_error(
                                    Errno::EIO,
                                    "fdatasync",
                                    "an error occurred during synchronization",
                                ),
                            }
                        }
                        _ => syscall_error(
                            Errno::EROFS,
                            "fdatasync",
                            "does not support special files for synchronization",
                        ),
                    }
                }
                _ => syscall_error(
                    Errno::EINVAL,
                    "fdatasync",
                    "fd is attached to an object which is unsuitable for synchronization",
                ),
            }
        } else {
            syscall_error(Errno::EBADF, "fdatasync", "invalid file descriptor")
        }
    }

    //------------------------------------SYNC_FILE_RANGE SYSCALL------------------------------------

    pub fn sync_file_range_syscall(
        &self,
        fd: i32,
        offset: isize,
        nbytes: isize,
        flags: u32,
    ) -> i32 {
        let checkedfd = self.get_filedescriptor(fd).unwrap();
        let mut unlocked_fd = checkedfd.write();
        if let Some(filedesc_enum) = &mut *unlocked_fd {
            match filedesc_enum {
                File(ref mut normalfile_filedesc_obj) => {
                    let inodeobj = FS_METADATA
                        .inodetable
                        .get(&normalfile_filedesc_obj.inode)
                        .unwrap();
                    match &*inodeobj {
                        Inode::File(_) => {
                            // This code segment obtains the file object associated with the specified inode from FILEOBJECTTABLE.
                            // It calls 'sync_file_range' on this file object, where initially the flags are validated, returning -EINVAL for incorrect flags.
                            // If the flags are correct, libc::sync_file_range is invoked; if it fails (returns -1), 'from_discriminant' function handles the error code.

                            let fobj = FILEOBJECTTABLE.get(&normalfile_filedesc_obj.inode).unwrap();
                            let result = fobj.sync_file_range(offset, nbytes, flags);
                            if result == 0 || result == -(EINVAL as i32) {
                                return result;
                            }
                            match Errno::from_discriminant(interface::get_errno()) {
                                Ok(i) => {
                                    return syscall_error(
                                        i,
                                        "sync_file_range",
                                        "The libc call to sync_file_range failed!",
                                    );
                                }
                                Err(()) => panic!("Unknown errno value from setsockopt returned!"),
                            };
                        }
                        _ => syscall_error(
                            Errno::ESPIPE,
                            "sync_file_range",
                            "does not support special files for synchronization",
                        ),
                    }
                }
                _ => syscall_error(
                    Errno::EBADF,
                    "sync_file_range",
                    "fd is attached to an object which is unsuitable for synchronization",
                ),
            }
        } else {
            syscall_error(Errno::EBADF, "sync_file_range", "invalid file descriptor")
        }
    }

    //------------------FTRUNCATE SYSCALL------------------

    pub fn ftruncate_syscall(&self, fd: i32, length: isize) -> i32 {
        let checkedfd = self.get_filedescriptor(fd).unwrap();
        let unlocked_fd = checkedfd.read();
        if let Some(filedesc_enum) = &*unlocked_fd {
            match filedesc_enum {
                // only proceed when fd references a regular file
                File(normalfile_filedesc_obj) => {
                    if is_rdonly(normalfile_filedesc_obj.flags) {
                        return syscall_error(
                            Errno::EBADF,
                            "ftruncate",
                            "specified file not open for writing",
                        );
                    }
                    let inodenum = normalfile_filedesc_obj.inode;
                    self._truncate_helper(inodenum, length, true)
                }
                _ => syscall_error(
                    Errno::EINVAL,
                    "ftruncate",
                    "fd does not reference a regular file",
                ),
            }
        } else {
            syscall_error(
                Errno::EBADF,
                "ftruncate",
                "fd is not a valid file descriptor",
            )
        }
    }

    //------------------TRUNCATE SYSCALL------------------
    pub fn truncate_syscall(&self, path: &str, length: isize) -> i32 {
        let truepath = normpath(convpath(path), self);

        //Walk the file tree to get inode from path
        if let Some(inodenum) = metawalk(truepath.as_path()) {
            self._truncate_helper(inodenum, length, false)
        } else {
            syscall_error(
                Errno::ENOENT,
                "truncate",
                "path does not refer to an existing file",
            )
        }
    }

    //------------------PIPE SYSCALL------------------
    pub fn pipe_syscall(&self, pipefd: &mut PipeArray) -> i32 {
        self.pipe2_syscall(pipefd, 0)
    }

    pub fn pipe2_syscall(&self, pipefd: &mut PipeArray, flags: i32) -> i32 {
        let flagsmask = O_CLOEXEC | O_NONBLOCK;
        let actualflags = flags & flagsmask;

        let pipe = interface::RustRfc::new(interface::new_pipe(PIPE_CAPACITY));

        // get an fd for each end of the pipe and set flags to RD_ONLY and WR_ONLY
        // append each to pipefds list

        let accflags = [O_RDONLY, O_WRONLY];
        for accflag in accflags {
            let (fd, guardopt) = self.get_next_fd(None);
            if fd < 0 {
                return fd;
            }
            let fdoption = &mut *guardopt.unwrap();

            let _insertval = fdoption.insert(Pipe(PipeDesc {
                pipe: pipe.clone(),
                flags: accflag | actualflags,
                advlock: interface::RustRfc::new(interface::AdvisoryLock::new()),
            }));

            match accflag {
                O_RDONLY => {
                    pipefd.readfd = fd;
                }
                O_WRONLY => {
                    pipefd.writefd = fd;
                }
                _ => panic!("How did you get here."),
            }
        }

        0 // success
    }

    //------------------GETDENTS SYSCALL------------------

    pub fn getdents_syscall(&self, fd: i32, dirp: *mut u8, bufsize: u32) -> i32 {
        let mut vec: Vec<(interface::ClippedDirent, Vec<u8>)> = Vec::new();

        // make sure bufsize is at least greater than size of a ClippedDirent struct
        if bufsize <= interface::CLIPPED_DIRENT_SIZE {
            return syscall_error(Errno::EINVAL, "getdents", "Result buffer is too small.");
        }

        let checkedfd = self.get_filedescriptor(fd).unwrap();
        let mut unlocked_fd = checkedfd.write();
        if let Some(filedesc_enum) = &mut *unlocked_fd {
            match filedesc_enum {
                // only proceed when fd represents a file
                File(ref mut normalfile_filedesc_obj) => {
                    let inodeobj = FS_METADATA
                        .inodetable
                        .get(&normalfile_filedesc_obj.inode)
                        .unwrap();

                    match &*inodeobj {
                        // only proceed when inode is a dir
                        Inode::Dir(dir_inode_obj) => {
                            let position = normalfile_filedesc_obj.position;
                            let mut bufcount = 0;
                            let mut curr_size;
                            let mut count = 0;
                            let mut temp_len;

                            // iterate over filename-inode pairs in dict
                            for (filename, inode) in dir_inode_obj
                                .filename_to_inode_dict
                                .clone()
                                .into_iter()
                                .skip(position)
                            {
                                // convert filename to a filename vector of u8
                                let mut vec_filename: Vec<u8> = filename.as_bytes().to_vec();
                                vec_filename.push(b'\0'); // make filename null-terminated

                                vec_filename.push(DT_UNKNOWN); // push DT_UNKNOWN as d_type (for now)
                                temp_len =
                                    interface::CLIPPED_DIRENT_SIZE + vec_filename.len() as u32; // get length of current filename vector for padding calculation

                                // pad filename vector to the next highest 8 byte boundary
                                for _ in 0..(temp_len + 7) / 8 * 8 - temp_len {
                                    vec_filename.push(00);
                                }

                                // the fixed dirent size and length of filename vector add up to total size
                                curr_size =
                                    interface::CLIPPED_DIRENT_SIZE + vec_filename.len() as u32;

                                bufcount += curr_size; // increment bufcount

                                // stop iteration if current bufcount exceeds argument bufsize
                                if bufcount > bufsize {
                                    bufcount = bufcount - curr_size; // decrement bufcount since current element is not actually written
                                    break;
                                }

                                // push properly constructed tuple to vector storing result
                                vec.push((
                                    interface::ClippedDirent {
                                        d_ino: inode as u64,
                                        d_off: bufcount as u64,
                                        d_reclen: curr_size as u16,
                                    },
                                    vec_filename,
                                ));
                                count += 1;
                            }
                            // update file position
                            normalfile_filedesc_obj.position = interface::rust_min(
                                position + count,
                                dir_inode_obj.filename_to_inode_dict.len(),
                            );

                            interface::pack_dirents(vec, dirp);
                            bufcount as i32 // return the number of bytes written
                        }
                        _ => syscall_error(
                            Errno::ENOTDIR,
                            "getdents",
                            "File descriptor does not refer to a directory.",
                        ),
                    }
                }
                // raise error when fd represents a socket, pipe, or stream
                _ => syscall_error(
                    Errno::ESPIPE,
                    "getdents",
                    "Cannot getdents since fd does not refer to a file.",
                ),
            }
        } else {
            syscall_error(Errno::EBADF, "getdents", "Invalid file descriptor")
        }
    }

    //------------------------------------GETCWD SYSCALL------------------------------------

    pub fn getcwd_syscall(&self, buf: *mut u8, bufsize: u32) -> i32 {
        let mut bytes: Vec<u8> = self.cwd.read().to_str().unwrap().as_bytes().to_vec();
        bytes.push(0u8); //Adding a null terminator to the end of the string
        let length = bytes.len();

        if (bufsize as usize) < length {
            return syscall_error(Errno::ERANGE, "getcwd", "the length (in bytes) of the absolute pathname of the current working directory exceeds the given size");
        }

        interface::fill(buf, length, &bytes);

        0 //getcwd has succeeded!;
    }

    //------------------SHMHELPERS----------------------

    pub fn rev_shm_find_index_by_addr(rev_shm: &Vec<(u32, i32)>, shmaddr: u32) -> Option<usize> {
        for (index, val) in rev_shm.iter().enumerate() {
            if val.0 == shmaddr as u32 {
                return Some(index);
            }
        }
        None
    }

    pub fn rev_shm_find_addrs_by_shmid(rev_shm: &Vec<(u32, i32)>, shmid: i32) -> Vec<u32> {
        let mut addrvec = Vec::new();
        for val in rev_shm.iter() {
            if val.1 == shmid as i32 {
                addrvec.push(val.0);
            }
        }

        return addrvec;
    }

    pub fn search_for_addr_in_region(
        rev_shm: &Vec<(u32, i32)>,
        search_addr: u32,
    ) -> Option<(u32, i32)> {
        let metadata = &SHM_METADATA;
        for val in rev_shm.iter() {
            let addr = val.0;
            let shmid = val.1;
            if let Some(segment) = metadata.shmtable.get_mut(&shmid) {
                let range = addr..(addr + segment.size as u32);
                if range.contains(&search_addr) {
                    return Some((addr, shmid));
                }
            }
        }
        None
    }

    //------------------SHMGET SYSCALL------------------

    pub fn shmget_syscall(&self, key: i32, size: usize, shmflg: i32) -> i32 {
        if key == IPC_PRIVATE {
            return syscall_error(Errno::ENOENT, "shmget", "IPC_PRIVATE not implemented");
        }
        let shmid: i32;
        let metadata = &SHM_METADATA;

        match metadata.shmkeyidtable.entry(key) {
            interface::RustHashEntry::Occupied(occupied) => {
                if (IPC_CREAT | IPC_EXCL) == (shmflg & (IPC_CREAT | IPC_EXCL)) {
                    return syscall_error(
                        Errno::EEXIST,
                        "shmget",
                        "key already exists and IPC_CREAT and IPC_EXCL were used",
                    );
                }
                shmid = *occupied.get();
            }
            interface::RustHashEntry::Vacant(vacant) => {
                if 0 == (shmflg & IPC_CREAT) {
                    return syscall_error(
                        Errno::ENOENT,
                        "shmget",
                        "tried to use a key that did not exist, and IPC_CREAT was not specified",
                    );
                }

                if (size as u32) < SHMMIN || (size as u32) > SHMMAX {
                    return syscall_error(
                        Errno::EINVAL,
                        "shmget",
                        "Size is less than SHMMIN or more than SHMMAX",
                    );
                }

                shmid = metadata.new_keyid();
                vacant.insert(shmid);
                let mode = (shmflg & 0x1FF) as u16; // mode is 9 least signficant bits of shmflag, even if we dont really do anything with them

                let segment = new_shm_segment(
                    key,
                    size,
                    self.cageid as u32,
                    DEFAULT_UID,
                    DEFAULT_GID,
                    mode,
                );
                metadata.shmtable.insert(shmid, segment);
            }
        };
        shmid // return the shmid
    }

    //------------------SHMAT SYSCALL------------------

    pub fn shmat_syscall(&self, shmid: i32, shmaddr: *mut u8, shmflg: i32) -> i32 {
        let metadata = &SHM_METADATA;
        let prot: i32;
        if let Some(mut segment) = metadata.shmtable.get_mut(&shmid) {
            if 0 != (shmflg & SHM_RDONLY) {
                prot = PROT_READ;
            } else {
                prot = PROT_READ | PROT_WRITE;
            }
            let mut rev_shm = self.rev_shm.lock();
            rev_shm.push((shmaddr as u32, shmid));
            drop(rev_shm);

            // update semaphores
            if !segment.semaphor_offsets.is_empty() {
                // lets just look at the first cage in the set, since we only need to grab the ref from one
                if let Some(cageid) = segment
                    .attached_cages
                    .clone()
                    .into_read_only()
                    .keys()
                    .next()
                {
                    let cage2 = interface::cagetable_getref(*cageid);
                    let cage2_rev_shm = cage2.rev_shm.lock();
                    let addrs = Self::rev_shm_find_addrs_by_shmid(&cage2_rev_shm, shmid); // find all the addresses assoc. with shmid
                    for offset in segment.semaphor_offsets.iter() {
                        let sementry = cage2.sem_table.get(&(addrs[0] + *offset)).unwrap().clone(); //add  semaphors into semtable at addr + offsets
                        self.sem_table.insert(shmaddr as u32 + *offset, sementry);
                    }
                }
            }

            segment.map_shm(shmaddr, prot, self.cageid)
        } else {
            syscall_error(Errno::EINVAL, "shmat", "Invalid shmid value")
        }
    }

    //------------------SHMDT SYSCALL------------------

    pub fn shmdt_syscall(&self, shmaddr: *mut u8) -> i32 {
        let metadata = &SHM_METADATA;
        let mut rm = false;
        let mut rev_shm = self.rev_shm.lock();
        let rev_shm_index = Self::rev_shm_find_index_by_addr(&rev_shm, shmaddr as u32);

        if let Some(index) = rev_shm_index {
            let shmid = rev_shm[index].1;
            match metadata.shmtable.entry(shmid) {
                interface::RustHashEntry::Occupied(mut occupied) => {
                    let segment = occupied.get_mut();

                    // update semaphores
                    for offset in segment.semaphor_offsets.iter() {
                        self.sem_table.remove(&(shmaddr as u32 + *offset));
                    }

                    segment.unmap_shm(shmaddr, self.cageid);

                    if segment.rmid && segment.shminfo.shm_nattch == 0 {
                        rm = true;
                    }
                    rev_shm.swap_remove(index);

                    if rm {
                        let key = segment.key;
                        occupied.remove_entry();
                        metadata.shmkeyidtable.remove(&key);
                    }

                    return shmid; //NaCl relies on this non-posix behavior of returning the shmid on success
                }
                interface::RustHashEntry::Vacant(_) => {
                    panic!("Inode not created for some reason");
                }
            };
        } else {
            return syscall_error(
                Errno::EINVAL,
                "shmdt",
                "No shared memory segment at shmaddr",
            );
        }
    }

    //------------------SHMCTL SYSCALL------------------

    pub fn shmctl_syscall(&self, shmid: i32, cmd: i32, buf: Option<&mut ShmidsStruct>) -> i32 {
        let metadata = &SHM_METADATA;

        if let Some(mut segment) = metadata.shmtable.get_mut(&shmid) {
            match cmd {
                IPC_STAT => {
                    *buf.unwrap() = segment.shminfo;
                }
                IPC_RMID => {
                    segment.rmid = true;
                    segment.shminfo.shm_perm.mode |= SHM_DEST as u16;
                    if segment.shminfo.shm_nattch == 0 {
                        let key = segment.key;
                        drop(segment);
                        metadata.shmtable.remove(&shmid);
                        metadata.shmkeyidtable.remove(&key);
                    }
                }
                _ => {
                    return syscall_error(
                        Errno::EINVAL,
                        "shmctl",
                        "Arguments provided do not match implemented parameters",
                    );
                }
            }
        } else {
            return syscall_error(Errno::EINVAL, "shmctl", "Invalid identifier");
        }

        0 //shmctl has succeeded!
    }

    //------------------MUTEX SYSCALLS------------------

    pub fn mutex_create_syscall(&self) -> i32 {
        let mut mutextable = self.mutex_table.write();
        let mut index_option = None;
        for i in 0..mutextable.len() {
            if mutextable[i].is_none() {
                index_option = Some(i);
                break;
            }
        }

        let index = if let Some(ind) = index_option {
            ind
        } else {
            mutextable.push(None);
            mutextable.len() - 1
        };

        let mutex_result = interface::RawMutex::create();
        match mutex_result {
            Ok(mutex) => {
                mutextable[index] = Some(interface::RustRfc::new(mutex));
                index as i32
            }
            Err(_) => match Errno::from_discriminant(interface::get_errno()) {
                Ok(i) => syscall_error(
                    i,
                    "mutex_create",
                    "The libc call to pthread_mutex_init failed!",
                ),
                Err(()) => panic!("Unknown errno value from pthread_mutex_init returned!"),
            },
        }
    }

    pub fn mutex_destroy_syscall(&self, mutex_handle: i32) -> i32 {
        let mut mutextable = self.mutex_table.write();
        if mutex_handle < mutextable.len() as i32
            && mutex_handle >= 0
            && mutextable[mutex_handle as usize].is_some()
        {
            mutextable[mutex_handle as usize] = None;
            0
        } else {
            //undefined behavior
            syscall_error(
                Errno::EBADF,
                "mutex_destroy",
                "Mutex handle does not refer to a valid mutex!",
            )
        }
        //the RawMutex is destroyed on Drop

        //this is currently assumed to always succeed, as the man page does not list possible
        //errors for pthread_mutex_destroy
    }

    pub fn mutex_lock_syscall(&self, mutex_handle: i32) -> i32 {
        let mutextable = self.mutex_table.read();
        if mutex_handle < mutextable.len() as i32
            && mutex_handle >= 0
            && mutextable[mutex_handle as usize].is_some()
        {
            let clonedmutex = mutextable[mutex_handle as usize].as_ref().unwrap().clone();
            drop(mutextable);
            let retval = clonedmutex.lock();

            if retval < 0 {
                match Errno::from_discriminant(interface::get_errno()) {
                    Ok(i) => {
                        return syscall_error(
                            i,
                            "mutex_lock",
                            "The libc call to pthread_mutex_lock failed!",
                        );
                    }
                    Err(()) => panic!("Unknown errno value from pthread_mutex_lock returned!"),
                };
            }

            retval
        } else {
            //undefined behavior
            syscall_error(
                Errno::EBADF,
                "mutex_lock",
                "Mutex handle does not refer to a valid mutex!",
            )
        }
    }

    pub fn mutex_trylock_syscall(&self, mutex_handle: i32) -> i32 {
        let mutextable = self.mutex_table.read();
        if mutex_handle < mutextable.len() as i32
            && mutex_handle >= 0
            && mutextable[mutex_handle as usize].is_some()
        {
            let clonedmutex = mutextable[mutex_handle as usize].as_ref().unwrap().clone();
            drop(mutextable);
            let retval = clonedmutex.trylock();

            if retval < 0 {
                match Errno::from_discriminant(interface::get_errno()) {
                    Ok(i) => {
                        return syscall_error(
                            i,
                            "mutex_trylock",
                            "The libc call to pthread_mutex_trylock failed!",
                        );
                    }
                    Err(()) => panic!("Unknown errno value from pthread_mutex_trylock returned!"),
                };
            }

            retval
        } else {
            //undefined behavior
            syscall_error(
                Errno::EBADF,
                "mutex_trylock",
                "Mutex handle does not refer to a valid mutex!",
            )
        }
    }

    pub fn mutex_unlock_syscall(&self, mutex_handle: i32) -> i32 {
        let mutextable = self.mutex_table.read();
        if mutex_handle < mutextable.len() as i32
            && mutex_handle >= 0
            && mutextable[mutex_handle as usize].is_some()
        {
            let clonedmutex = mutextable[mutex_handle as usize].as_ref().unwrap().clone();
            drop(mutextable);
            let retval = clonedmutex.unlock();

            if retval < 0 {
                match Errno::from_discriminant(interface::get_errno()) {
                    Ok(i) => {
                        return syscall_error(
                            i,
                            "mutex_unlock",
                            "The libc call to pthread_mutex_unlock failed!",
                        );
                    }
                    Err(()) => panic!("Unknown errno value from pthread_mutex_unlock returned!"),
                };
            }

            retval
        } else {
            //undefined behavior
            syscall_error(
                Errno::EBADF,
                "mutex_unlock",
                "Mutex handle does not refer to a valid mutex!",
            )
        }
    }

    //------------------CONDVAR SYSCALLS------------------

    pub fn cond_create_syscall(&self) -> i32 {
        let mut cvtable = self.cv_table.write();
        let mut index_option = None;
        for i in 0..cvtable.len() {
            if cvtable[i].is_none() {
                index_option = Some(i);
                break;
            }
        }

        let index = if let Some(ind) = index_option {
            ind
        } else {
            cvtable.push(None);
            cvtable.len() - 1
        };

        let cv_result = interface::RawCondvar::create();
        match cv_result {
            Ok(cv) => {
                cvtable[index] = Some(interface::RustRfc::new(cv));
                index as i32
            }
            Err(_) => match Errno::from_discriminant(interface::get_errno()) {
                Ok(i) => syscall_error(
                    i,
                    "cond_create",
                    "The libc call to pthread_cond_init failed!",
                ),
                Err(()) => panic!("Unknown errno value from pthread_cond_init returned!"),
            },
        }
    }

    pub fn cond_destroy_syscall(&self, cv_handle: i32) -> i32 {
        let mut cvtable = self.cv_table.write();
        if cv_handle < cvtable.len() as i32
            && cv_handle >= 0
            && cvtable[cv_handle as usize].is_some()
        {
            cvtable[cv_handle as usize] = None;
            0
        } else {
            //undefined behavior
            syscall_error(
                Errno::EBADF,
                "cond_destroy",
                "Condvar handle does not refer to a valid condvar!",
            )
        }
        //the RawCondvar is destroyed on Drop

        //this is currently assumed to always succeed, as the man page does not list possible
        //errors for pthread_cv_destroy
    }

    pub fn cond_signal_syscall(&self, cv_handle: i32) -> i32 {
        let cvtable = self.cv_table.read();
        if cv_handle < cvtable.len() as i32
            && cv_handle >= 0
            && cvtable[cv_handle as usize].is_some()
        {
            let clonedcv = cvtable[cv_handle as usize].as_ref().unwrap().clone();
            drop(cvtable);
            let retval = clonedcv.signal();

            if retval < 0 {
                match Errno::from_discriminant(interface::get_errno()) {
                    Ok(i) => {
                        return syscall_error(
                            i,
                            "cond_signal",
                            "The libc call to pthread_cond_signal failed!",
                        );
                    }
                    Err(()) => panic!("Unknown errno value from pthread_cond_signal returned!"),
                };
            }

            retval
        } else {
            //undefined behavior
            syscall_error(
                Errno::EBADF,
                "cond_signal",
                "Condvar handle does not refer to a valid condvar!",
            )
        }
    }

    pub fn cond_broadcast_syscall(&self, cv_handle: i32) -> i32 {
        let cvtable = self.cv_table.read();
        if cv_handle < cvtable.len() as i32
            && cv_handle >= 0
            && cvtable[cv_handle as usize].is_some()
        {
            let clonedcv = cvtable[cv_handle as usize].as_ref().unwrap().clone();
            drop(cvtable);
            let retval = clonedcv.broadcast();

            if retval < 0 {
                match Errno::from_discriminant(interface::get_errno()) {
                    Ok(i) => {
                        return syscall_error(
                            i,
                            "cond_broadcast",
                            "The libc call to pthread_cond_broadcast failed!",
                        );
                    }
                    Err(()) => panic!("Unknown errno value from pthread_cond_broadcast returned!"),
                };
            }

            retval
        } else {
            //undefined behavior
            syscall_error(
                Errno::EBADF,
                "cond_broadcast",
                "Condvar handle does not refer to a valid condvar!",
            )
        }
    }

    pub fn cond_wait_syscall(&self, cv_handle: i32, mutex_handle: i32) -> i32 {
        let cvtable = self.cv_table.read();
        if cv_handle < cvtable.len() as i32
            && cv_handle >= 0
            && cvtable[cv_handle as usize].is_some()
        {
            let clonedcv = cvtable[cv_handle as usize].as_ref().unwrap().clone();
            drop(cvtable);

            let mutextable = self.mutex_table.read();
            if mutex_handle < mutextable.len() as i32
                && mutex_handle >= 0
                && mutextable[mutex_handle as usize].is_some()
            {
                let clonedmutex = mutextable[mutex_handle as usize].as_ref().unwrap().clone();
                drop(mutextable);
                let retval = clonedcv.wait(&*clonedmutex);

                // if the cancel status is set in the cage, we trap around a cancel point
                // until the individual thread is signaled to cancel itself
                if self
                    .cancelstatus
                    .load(interface::RustAtomicOrdering::Relaxed)
                {
                    loop {
                        interface::cancelpoint(self.cageid);
                    } // we check cancellation status here without letting the function return
                }

                if retval < 0 {
                    match Errno::from_discriminant(interface::get_errno()) {
                        Ok(i) => {
                            return syscall_error(
                                i,
                                "cond_wait",
                                "The libc call to pthread_cond_wait failed!",
                            );
                        }
                        Err(()) => panic!("Unknown errno value from pthread_cond_wait returned!"),
                    };
                }

                retval
            } else {
                //undefined behavior
                syscall_error(
                    Errno::EBADF,
                    "cond_wait",
                    "Mutex handle does not refer to a valid mutex!",
                )
            }
        } else {
            //undefined behavior
            syscall_error(
                Errno::EBADF,
                "cond_wait",
                "Condvar handle does not refer to a valid condvar!",
            )
        }
    }

    pub fn cond_timedwait_syscall(
        &self,
        cv_handle: i32,
        mutex_handle: i32,
        time: interface::RustDuration,
    ) -> i32 {
        let cvtable = self.cv_table.read();
        if cv_handle < cvtable.len() as i32
            && cv_handle >= 0
            && cvtable[cv_handle as usize].is_some()
        {
            let clonedcv = cvtable[cv_handle as usize].as_ref().unwrap().clone();
            drop(cvtable);

            let mutextable = self.mutex_table.read();
            if mutex_handle < mutextable.len() as i32
                && mutex_handle >= 0
                && mutextable[mutex_handle as usize].is_some()
            {
                let clonedmutex = mutextable[mutex_handle as usize].as_ref().unwrap().clone();
                drop(mutextable);
                let retval = clonedcv.timedwait(&*clonedmutex, time);
                if retval < 0 {
                    match Errno::from_discriminant(interface::get_errno()) {
                        Ok(i) => {
                            return syscall_error(
                                i,
                                "cond_wait",
                                "The libc call to pthread_cond_wait failed!",
                            );
                        }
                        Err(()) => panic!("Unknown errno value from pthread_cond_wait returned!"),
                    };
                }

                retval
            } else {
                //undefined behavior
                syscall_error(
                    Errno::EBADF,
                    "cond_wait",
                    "Mutex handle does not refer to a valid mutex!",
                )
            }
        } else {
            //undefined behavior
            syscall_error(
                Errno::EBADF,
                "cond_wait",
                "Condvar handle does not refer to a valid condvar!",
            )
        }
    }

    //------------------SEMAPHORE SYSCALLS------------------
    /*
     *  Initialize semaphore object SEM to value
     *  pshared used to indicate whether the semaphore is shared in threads (when equals to 0)
     *  or shared between processes (when nonzero)
     */
    pub fn sem_init_syscall(&self, sem_handle: u32, pshared: i32, value: u32) -> i32 {
        // Boundary check
        if value > SEM_VALUE_MAX {
            return syscall_error(Errno::EINVAL, "sem_init", "value exceeds SEM_VALUE_MAX");
        }

        let metadata = &SHM_METADATA;
        let is_shared = pshared != 0;

        // Iterate semaphore table, if semaphore is already initialzed return error
        let semtable = &self.sem_table;

        // Will initialize only it's new
        if !semtable.contains_key(&sem_handle) {
            let new_semaphore =
                interface::RustRfc::new(interface::RustSemaphore::new(value, is_shared));
            semtable.insert(sem_handle, new_semaphore.clone());

            if is_shared {
                let rev_shm = self.rev_shm.lock();
                // if its shared and exists in an existing mapping we need to add it to other cages
                if let Some((mapaddr, shmid)) =
                    Self::search_for_addr_in_region(&rev_shm, sem_handle)
                {
                    let offset = mapaddr - sem_handle;
                    if let Some(segment) = metadata.shmtable.get_mut(&shmid) {
                        for cageid in segment.attached_cages.clone().into_read_only().keys() {
                            // iterate through all cages with segment attached and add semaphor in segments at attached addr + offset
                            let cage = interface::cagetable_getref(*cageid);
                            let addrs = Self::rev_shm_find_addrs_by_shmid(&rev_shm, shmid);
                            for addr in addrs.iter() {
                                cage.sem_table.insert(addr + offset, new_semaphore.clone());
                            }
                        }
                        segment.semaphor_offsets.insert(offset);
                    }
                }
            }
            return 0;
        }

        return syscall_error(Errno::EBADF, "sem_init", "semaphore already initialized");
    }

    pub fn sem_wait_syscall(&self, sem_handle: u32) -> i32 {
        let semtable = &self.sem_table;
        // Check whether semaphore exists
        if let Some(sementry) = semtable.get_mut(&sem_handle) {
            let semaphore = sementry.clone();
            drop(sementry);
            semaphore.lock();
        } else {
            return syscall_error(Errno::EINVAL, "sem_wait", "sem is not a valid semaphore");
        }
        return 0;
    }

    pub fn sem_post_syscall(&self, sem_handle: u32) -> i32 {
        let semtable = &self.sem_table;
        if let Some(sementry) = semtable.get_mut(&sem_handle) {
            let semaphore = sementry.clone();
            drop(sementry);
            if !semaphore.unlock() {
                return syscall_error(
                    Errno::EOVERFLOW,
                    "sem_post",
                    "The maximum allowable value for a semaphore would be exceeded",
                );
            }
        } else {
            return syscall_error(Errno::EINVAL, "sem_wait", "sem is not a valid semaphore");
        }
        return 0;
    }

    pub fn sem_destroy_syscall(&self, sem_handle: u32) -> i32 {
        let metadata = &SHM_METADATA;

        let semtable = &self.sem_table;
        // remove entry from semaphore table
        if let Some(sementry) = semtable.remove(&sem_handle) {
            if sementry
                .1
                .is_shared
                .load(interface::RustAtomicOrdering::Relaxed)
            {
                // if its shared we'll need to remove it from other attachments
                let rev_shm = self.rev_shm.lock();
                if let Some((mapaddr, shmid)) =
                    Self::search_for_addr_in_region(&rev_shm, sem_handle)
                {
                    // find all segments that contain semaphore
                    let offset = mapaddr - sem_handle;
                    if let Some(segment) = metadata.shmtable.get_mut(&shmid) {
                        for cageid in segment.attached_cages.clone().into_read_only().keys() {
                            // iterate through all cages containing segment
                            let cage = interface::cagetable_getref(*cageid);
                            let addrs = Self::rev_shm_find_addrs_by_shmid(&rev_shm, shmid);
                            for addr in addrs.iter() {
                                cage.sem_table.remove(&(addr + offset)); //remove semapoores at attached addresses + the offset
                            }
                        }
                    }
                }
            }
            return 0;
        } else {
            return syscall_error(Errno::EINVAL, "sem_destroy", "sem is not a valid semaphore");
        }
    }

    /*
     * Take only sem_t *sem as argument, and return int *sval
     */
    pub fn sem_getvalue_syscall(&self, sem_handle: u32) -> i32 {
        let semtable = &self.sem_table;
        if let Some(sementry) = semtable.get_mut(&sem_handle) {
            let semaphore = sementry.clone();
            drop(sementry);
            return semaphore.get_value();
        }
        return syscall_error(
            Errno::EINVAL,
            "sem_getvalue",
            "sem is not a valid semaphore",
        );
    }

    pub fn sem_trywait_syscall(&self, sem_handle: u32) -> i32 {
        let semtable = &self.sem_table;
        // Check whether semaphore exists
        if let Some(sementry) = semtable.get_mut(&sem_handle) {
            let semaphore = sementry.clone();
            drop(sementry);
            if !semaphore.trylock() {
                return syscall_error(
                    Errno::EAGAIN,
                    "sem_trywait",
                    "The operation could not be performed without blocking",
                );
            }
        } else {
            return syscall_error(Errno::EINVAL, "sem_trywait", "sem is not a valid semaphore");
        }
        return 0;
    }

    pub fn sem_timedwait_syscall(&self, sem_handle: u32, time: interface::RustDuration) -> i32 {
        let abstime = libc::timespec {
            tv_sec: time.as_secs() as i64,
            tv_nsec: (time.as_nanos() % 1000000000) as i64,
        };
        if abstime.tv_nsec < 0 {
            return syscall_error(Errno::EINVAL, "sem_timedwait", "Invalid timedout");
        }
        let semtable = &self.sem_table;
        // Check whether semaphore exists
        if let Some(sementry) = semtable.get_mut(&sem_handle) {
            let semaphore = sementry.clone();
            drop(sementry);
            if !semaphore.timedlock(time) {
                return syscall_error(
                    Errno::ETIMEDOUT,
                    "sem_timedwait",
                    "The call timed out before the semaphore could be locked",
                );
            }
        } else {
            return syscall_error(
                Errno::EINVAL,
                "sem_timedwait",
                "sem is not a valid semaphore",
            );
        }
        return 0;
    }

    // We're directly patching in the libc futex call for experimentation with lind-wasm
    // this should allow us to use the nptl data structures such as mutexes and condvars directly
    // as opposed to lind-nacl's individual implementations
    //
    // to perform this we just directly pass futex's var args as unsigned 32 bit integers to syscall() with SYS_futex
    pub fn futex_syscall(&self, uaddr: u64, futex_op: u32, val: u32, val2: u32, uaddr2: u32, val3: u32) -> i32 { 
        let retval = interface::libc_futex(uaddr, futex_op, val, val2, uaddr2, val3);
        return retval;
    }

}
