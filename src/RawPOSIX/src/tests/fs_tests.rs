#[allow(unused_parens)]
#[cfg(test)]
pub mod fs_tests {

    use super::super::*;
    use fdtables::{translate_virtual_fd};
    use sysdefs::{
        constants::{
            err_const::get_errno,
            fs_const::{PAGESIZE, SHMMAX, S_IRWXA},
            sys_const::{DEFAULT_GID, DEFAULT_UID},
            Errno,
        },
        data::{
            fs_struct::{
                ClippedDirent, FSData, PipeArray, ShmidsStruct, SockPair, StatData, CLIPPED_DIRENT_SIZE,
            },
            net_struct::{GenSockaddr, SockaddrV4},
        },
    };
    use crate::interface;
    use crate::safeposix::syscalls::fs_calls::*;
    use crate::safeposix::{cage::*, dispatcher::*, filesystem};

    use libc::*;
    use libc::{c_void, O_DIRECTORY};
    pub use std::ffi::CStr as RustCStr;
    use std::fs::OpenOptions;
    use std::mem;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    pub fn ut_lind_fs_simple() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        // Create a test directory
        let test_root = "/test_root";
        let res = cage.mkdir_syscall(test_root, 0o755);
        assert!(res == 0 || res == -libc::EEXIST);

        // Verify access
        assert_eq!(cage.access_syscall(test_root, F_OK), 0);
        assert_eq!(cage.access_syscall(test_root, X_OK | R_OK), 0);

        let mut statdata2 = StatData::default();

        // Get stats for the test directory
        assert_eq!(cage.stat_syscall(test_root, &mut statdata2), 0);

        // Since the directory is newly created and empty, st_nlink should be 2
        assert_eq!(statdata2.st_nlink, 2); // . and ..

        // Check that st_size is greater than or equal to 4096
        assert!(
            statdata2.st_size >= 4096,
            "Expected st_size >= 4096, got {}",
            statdata2.st_size
        );

        // Clean up
        assert_eq!(cage.rmdir_syscall(test_root), 0);

        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);

        lindrustfinalize();
    }

    #[test]
    pub fn rdwrtest() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        let fd = cage.open_syscall("/foobar", O_CREAT | O_TRUNC | O_RDWR, S_IRWXA);
        assert!(fd >= 0);

        assert_eq!(cage.write_syscall(fd, str2cbuf("hello there!"), 12), 12);

        assert_eq!(cage.lseek_syscall(fd, 0, SEEK_SET), 0);
        let mut read_buf1 = sizecbuf(5);
        assert_eq!(cage.read_syscall(fd, read_buf1.as_mut_ptr(), 5), 5);
        assert_eq!(cbuf2str(&read_buf1), "hello");

        assert_eq!(cage.write_syscall(fd, str2cbuf(" world"), 6), 6);

        assert_eq!(cage.lseek_syscall(fd, 0, SEEK_SET), 0);
        let mut read_buf2 = sizecbuf(12);
        assert_eq!(cage.read_syscall(fd, read_buf2.as_mut_ptr(), 12), 12);
        assert_eq!(cbuf2str(&read_buf2), "hello world!");

        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);

        lindrustfinalize();
    }

    #[test]
    pub fn prdwrtest() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        let fd = cage.open_syscall("/foobar2", O_CREAT | O_TRUNC | O_RDWR, S_IRWXA);
        assert!(fd >= 0);

        assert_eq!(cage.pwrite_syscall(fd, str2cbuf("hello there!"), 12, 0), 12);

        let mut read_buf1 = sizecbuf(5);
        assert_eq!(cage.pread_syscall(fd, read_buf1.as_mut_ptr(), 5, 0), 5);
        assert_eq!(cbuf2str(&read_buf1), "hello");

        assert_eq!(cage.pwrite_syscall(fd, str2cbuf(" world"), 6, 5), 6);

        let mut read_buf2 = sizecbuf(12);
        assert_eq!(cage.pread_syscall(fd, read_buf2.as_mut_ptr(), 12, 0), 12);
        assert_eq!(cbuf2str(&read_buf2), "hello world!");

        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    // #[test]
    pub fn chardevtest() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        let fd = cage.open_syscall("/dev/zero", O_RDWR, S_IRWXA);
        assert!(fd >= 0);

        assert_eq!(
            cage.pwrite_syscall(
                fd,
                str2cbuf("Lorem ipsum dolor sit amet, consectetur adipiscing elit"),
                55,
                0
            ),
            55
        );

        let mut read_bufzero = sizecbuf(1000);
        assert_eq!(
            cage.pread_syscall(fd, read_bufzero.as_mut_ptr(), 1000, 0),
            1000
        );
        assert_eq!(
            cbuf2str(&read_bufzero),
            std::iter::repeat("\0")
                .take(1000)
                .collect::<String>()
                .as_str()
        );

        assert_eq!(cage.chdir_syscall("dev"), 0);
        assert_eq!(cage.close_syscall(fd), 0);

        let fd2 = cage.open_syscall("./urandom", O_RDWR, S_IRWXA);
        assert!(fd2 >= 0);
        let mut read_bufrand = sizecbuf(1000);
        assert_eq!(
            cage.read_syscall(fd2, read_bufrand.as_mut_ptr(), 1000),
            1000
        );
        assert_eq!(cage.close_syscall(fd2), 0);
        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_exec_cloexec() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);
        let mut uselessstatdata = StatData::default();

        let fd1 = cage.open_syscall(
            "/cloexecuted",
            O_CREAT | O_TRUNC | O_RDWR | O_CLOEXEC,
            S_IRWXA,
        );
        let fd2 = cage.open_syscall("/cloexekept", O_CREAT | O_TRUNC | O_RDWR, S_IRWXA);
        assert!(fd1 > 0);
        assert!(fd2 > 0);
        assert_eq!(cage.fstat_syscall(fd1, &mut uselessstatdata), 0);
        assert_eq!(cage.fstat_syscall(fd2, &mut uselessstatdata), 0);

        assert_eq!(cage.exec_syscall(), 0);

        let execcage = interface::cagetable_getref(2);
        assert_eq!(
            execcage.fstat_syscall(fd1, &mut uselessstatdata),
            -(Errno::EBADF as i32)
        );
        assert_eq!(execcage.fstat_syscall(fd2, &mut uselessstatdata), 0);

        assert_eq!(execcage.close_syscall(fd2), 0);
        assert_eq!(cage.unlink_syscall("/cloexecuted"), 0);
        assert_eq!(cage.unlink_syscall("/cloexekept"), 0);

        assert_eq!(
            execcage.exit_syscall(libc::EXIT_SUCCESS),
            libc::EXIT_SUCCESS
        );
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_shm() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);
        let key = 31337;
        let mut shmidstruct = ShmidsStruct::default();

        // shmget returns an identifier in shmid
        let shmid = cage.shmget_syscall(key, 1024, 0666 | IPC_CREAT);

        // shmat to attach to shared memory
        let shmatret = cage.shmat_syscall(shmid, 0xfffff000 as *mut u8, 0);

        assert_ne!(shmatret, -1);

        // get struct info
        let shmctlret1 = cage.shmctl_syscall(shmid, IPC_STAT, Some(&mut shmidstruct));

        assert_eq!(shmctlret1, 0);

        assert_eq!(shmidstruct.shm_nattch, 1);

        // mark the shared memory to be rmoved
        let shmctlret2 = cage.shmctl_syscall(shmid, IPC_RMID, None);

        assert_eq!(shmctlret2, 0);

        //detach from shared memory
        let shmdtret = cage.shmdt_syscall(0xfffff000 as *mut u8);

        assert_eq!(shmdtret, shmid); //NaCl requires shmdt to return the shmid, so this is non-posixy

        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_shm_pipe_communication() {
        // Acquire lock and init environment
        let _thelock = setup::lock_and_init();
        
        let cage = interface::cagetable_getref(1);

        // Create pipe
        let mut pipe_fds = PipeArray::default();
        assert_eq!(cage.pipe_syscall(&mut pipe_fds), 0);
        
        // Fork a child process
        assert_eq!(cage.fork_syscall(2), 0);

        let child = std::thread::spawn(move || {
            let cage2 = interface::cagetable_getref(2);
            
            // Child writes to pipe
            let write_data = "Hello from child!";
            assert_eq!(
                cage2.write_syscall(pipe_fds.writefd, write_data.as_ptr(), write_data.len()),
                write_data.len() as i32
            );

            assert_eq!(cage2.close_syscall(pipe_fds.writefd), 0);
            assert_eq!(cage2.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        });

        // Parent reads from pipe
        let mut read_buf = sizecbuf(100);
        let bytes_read = cage.read_syscall(pipe_fds.readfd, read_buf.as_mut_ptr(), 100);
        assert!(bytes_read > 0);
        assert_eq!(cbuf2str(&read_buf[..bytes_read as usize]), "Hello from child!");

        child.join().unwrap();
        assert_eq!(cage.close_syscall(pipe_fds.readfd), 0);
        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_simple_file_operations() {
        // Acquire lock and init environment
        let _thelock = setup::lock_and_init();
        println!("Test initialized with lock");
        
        // Initialize filesystem
        lindrustinit(0);
        println!("Filesystem initialized");
        
        // Get cage reference
        let cage = interface::cagetable_getref(1);
        println!("Got cage reference with id 1");
    
        // Create /tmp directory if it doesn't exist
        if cage.access_syscall("/tmp", F_OK) != 0 {
            println!("Creating /tmp directory");
            assert_eq!(cage.mkdir_syscall("/tmp", S_IRWXA), 0, "Failed to create /tmp directory");
        }
        assert_eq!(cage.access_syscall("/tmp", F_OK), 0, "Failed to access /tmp directory");
        println!("Verified /tmp directory exists and is accessible");
    
        // Create a test file
        let filepath = "/tmp/test_file.txt";
        println!("Creating test file at {}", filepath);
        
        // Clean up any existing file
        if cage.access_syscall(filepath, F_OK) == 0 {
            println!("Removing existing file");
            assert_eq!(cage.unlink_syscall(filepath), 0, "Failed to remove existing file");
        }
    
        // Create the file
        let fd = cage.open_syscall(filepath, O_CREAT | O_RDWR, S_IRWXA);
        assert!(fd >= 0, "Failed to create file: {} (errno: {})", fd, unsafe { *libc::__errno_location() });
        println!("Successfully created file with fd: {}", fd);
    
        // Write some data to the file
        let test_data = "Hello, World!";
        println!("Writing data: {}", test_data);
        let write_result = cage.write_syscall(fd, test_data.as_ptr(), test_data.len());
        assert_eq!(write_result, test_data.len() as i32, "Failed to write data");
        println!("Successfully wrote {} bytes", write_result);
    
        // Seek back to start of file
        println!("Seeking to start of file");
        assert_eq!(cage.lseek_syscall(fd, 0, SEEK_SET), 0, "Failed to seek to start of file");
    
        // Read the data back
        let mut read_buf = sizecbuf(100);
        println!("Reading data from file");
        let bytes_read = cage.read_syscall(fd, read_buf.as_mut_ptr(), 100);
        assert!(bytes_read > 0, "Failed to read from file: {}", bytes_read);
        println!("Read {} bytes", bytes_read);
    
        // Verify the content
        let read_content = cbuf2str(&read_buf[..bytes_read as usize]);
        println!("Read content: {}", read_content);
        assert_eq!(read_content, test_data, "Unexpected content read: {}", read_content);
    
        // Clean up
        println!("Cleaning up");
        assert_eq!(cage.close_syscall(fd), 0, "Failed to close file");
        assert_eq!(cage.unlink_syscall(filepath), 0, "Failed to unlink file");
        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        println!("Test completed successfully");
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_shm_socket_communication() {
        // Acquire lock and init environment
        let _thelock = setup::lock_and_init();
        
        let cage = interface::cagetable_getref(1);

        // Create socket pair
        let mut socketpair = SockPair::default();
        assert_eq!(
            Cage::socketpair_syscall(
                &cage.clone(),
                libc::AF_UNIX,
                libc::SOCK_STREAM,
                0,
                &mut socketpair
            ),
            0
        );

        // Fork a child process
        assert_eq!(cage.fork_syscall(2), 0);

        let child = std::thread::spawn(move || {
            let cage2 = interface::cagetable_getref(2);
            
            // Child sends message through socket
            let message = "Message from child via socket";
            assert_eq!(
                cage2.send_syscall(socketpair.sock1, message.as_ptr(), message.len(), 0),
                message.len() as i32
            );

            assert_eq!(cage2.close_syscall(socketpair.sock1), 0);
            assert_eq!(cage2.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        });

        // Parent receives message
        let mut recv_buf = sizecbuf(100);
        let bytes_received = cage.recv_syscall(socketpair.sock2, recv_buf.as_mut_ptr(), 100, 0);
        assert!(bytes_received > 0);
        assert_eq!(
            cbuf2str(&recv_buf[..bytes_received as usize]),
            "Message from child via socket"
        );

        child.join().unwrap();
        assert_eq!(cage.close_syscall(socketpair.sock2), 0);
        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_getpid_getppid() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage1 = interface::cagetable_getref(1);
        let pid1 = cage1.getpid_syscall();

        assert_eq!(cage1.fork_syscall(2), 0);

        let child = std::thread::spawn(move || {
            let cage2 = interface::cagetable_getref(2);
            let pid2 = cage2.getpid_syscall();
            let ppid2 = cage2.getppid_syscall();

            assert_ne!(pid2, pid1); // make sure the child and the parent have different pids
            assert_eq!(ppid2, pid1); // make sure the child's getppid is correct

            assert_eq!(cage2.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        });

        child.join().unwrap();
        assert_eq!(cage1.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_tmp_file_test() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        // Check if /tmp is there
        if cage.access_syscall("/tmp", F_OK) != 0 {
            assert_eq!(
                cage.mkdir_syscall("/tmp", S_IRWXA),
                0,
                "Failed to create /tmp directory"
            );
        }
        assert_eq!(cage.access_syscall("/tmp", F_OK), 0);
        // Open  file in /tmp
        let file_path = "/tmp/testfile";
        let fd = cage.open_syscall(file_path, O_CREAT | O_TRUNC | O_RDWR, S_IRWXA);

        assert_eq!(cage.write_syscall(fd, str2cbuf("Hello world"), 6), 6);
        assert_eq!(cage.close_syscall(fd), 0);
        // Explicitly delete the file to clean up
        assert_eq!(
            cage.unlink_syscall(file_path),
            0,
            "Failed to delete /tmp/testfile"
        );

        lindrustfinalize();

        // Init again
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);
        // Ensure /tmp is created again after reinitialization
        if cage.access_syscall("/tmp", F_OK) != 0 {
            assert_eq!(
                cage.mkdir_syscall("/tmp", S_IRWXA),
                0,
                "Failed to recreate /tmp directory"
            );
        }

        // Check if /tmp is there
        assert_eq!(cage.access_syscall("/tmp", F_OK), 0);
        // Check if file is still there (it shouldn't be, assert no)
        assert_eq!(cage.access_syscall(file_path, F_OK), -2);

        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_mkdir_empty_directory() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);
        let path = "";
        // Check for error when directory is empty
        assert_eq!(cage.mkdir_syscall(path, S_IRWXA), -(Errno::ENOENT as i32));
        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_mkdir_nonexisting_directory() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);
        let path = "/parentdir/dir";
        // Ensure the directories do not exist for clean environment setup
        // BUG: this rmdir needs to be recursive, we'll change this after we PR a new version of the lindfs tool
        // Clear the directory if it exists use _ to ignore the return value
        let _ = cage.rmdir_syscall("/parentdir/dir");
        let _ = cage.rmdir_syscall("/parentdir");
        // Check for error when both parent and child directories don't exist
        assert_eq!(cage.mkdir_syscall(path, S_IRWXA), -(Errno::ENOENT as i32));
        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_mkdir_existing_directory() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);
        let path = "/parentdir";
        // Create a parent directory
        cage.mkdir_syscall(path, S_IRWXA);
        // Check for error when the same directory is created again
        assert_eq!(cage.mkdir_syscall(path, S_IRWXA), -(Errno::EEXIST as i32));
        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_mkdir_invalid_modebits() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);
        let subdir_path = "/parentdir/dir";
        let path = "/parentdir";
        let invalid_mode = 0o77777; // Invalid mode bits

        // Remove the directory if it exists
        let _ = cage.rmdir_syscall(subdir_path);
        let _ = cage.rmdir_syscall(path);

        // Create the parent directory
        assert_eq!(cage.mkdir_syscall(path, S_IRWXA), 0);
        // Now try to create a subdirectory under the parent directory
        let c_subdir_path = std::ffi::CString::new(subdir_path).unwrap();
        let result = unsafe { libc::mkdir(c_subdir_path.as_ptr(), invalid_mode) };

        // Check if mkdir failed
        if result != 0 {
            let errno_val = get_errno();
            match errno_val {
                libc::EPERM => assert_eq!(
                    errno_val,
                    libc::EPERM,
                    "Expected EPERM for invalid mode bits"
                ),
                libc::EINVAL => assert_eq!(
                    errno_val,
                    libc::EINVAL,
                    "Expected EINVAL for invalid mode bits"
                ),
                libc::ENOENT => println!("No such file or directory (ENOENT)"),
                _ => panic!("Unexpected error code: {}", errno_val),
            }
        }

        // Clean up and finalize
        assert_eq!(cage.rmdir_syscall(path), 0);
        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_mkdir_success() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);
        let path = "/parentdir";
        // Clear the directory if it exists
        let _ = cage.rmdir_syscall("/parentdir/dir");
        let _ = cage.rmdir_syscall(path);

        // Create a parent directory
        assert_eq!(cage.mkdir_syscall(path, S_IRWXA), 0);

        // Get the stat data for the parent directory and check for inode link count to be 2 initially
        // Explanation: A newly created directory has a link count of 2:
        // 1. A self-link (.) pointing to itself.
        // 2. A link from the parent directory (in this case, the root directory).
        // Previously, this was incorrectly checked as 3, but the correct count is 2.
        let mut statdata = StatData::default();
        assert_eq!(cage.stat_syscall(path, &mut statdata), 0);
        assert_eq!(statdata.st_nlink, 2); // Corrected from 3 to 2

        // Create a child directory inside the parent directory with valid mode bits
        assert_eq!(cage.mkdir_syscall("/parentdir/dir", S_IRWXA), 0);

        // Get the stat data for the child directory and check for inode link count to be 2 initially
        // Explanation: Similar to the parent directory, the newly created child directory will also
        // have a link count of 2:
        // 1. A self-link (.).
        // 2. A link (..) back to the parent directory (/parentdir).
        let mut statdata2 = StatData::default();
        assert_eq!(cage.stat_syscall("/parentdir/dir", &mut statdata2), 0);
        assert_eq!(statdata2.st_nlink, 2); // Child directory should have link count of 2

        // Get the stat data for the parent directory and check for inode link count to be 3 now
        // Explanation: After creating the child directory (/parentdir/dir), the parent directory's
        // link count increases by 1 because the child directory's (..) entry points back to the parent.
        // Initially, the parent had a link count of 2, but after adding the child directory, it becomes 3.
        // Previously, this was incorrectly checked as 4, but the correct count is 3.
        let mut statdata3 = StatData::default();
        assert_eq!(cage.stat_syscall(path, &mut statdata3), 0);
        assert_eq!(statdata3.st_nlink, 3); // Corrected from 4 to 3

        // Clean up and finalize
        assert_eq!(cage.rmdir_syscall("/parentdir/dir"), 0);
        assert_eq!(cage.rmdir_syscall(path), 0);
        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_mkdir_using_symlink() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);
        // Delete the files if it exists
        let _ = cage.unlink_syscall("/symlinkFile");
        let _ = cage.unlink_syscall("/originalFile");
        // Create a file which will be referred to as originalFile
        let fd = cage.open_syscall("/originalFile", O_CREAT | O_EXCL | O_WRONLY, S_IRWXA);
        assert_eq!(cage.write_syscall(fd, str2cbuf("hi"), 2), 2);

        // Create a link between two files where the symlinkFile is originally not
        // present But while linking, symlinkFile will get created
        assert_eq!(cage.link_syscall("/originalFile", "/symlinkFile"), 0);

        // Check for error while creating the symlinkFile again as it would already be
        // created while linking the two files above.
        assert_eq!(
            cage.mkdir_syscall("/symlinkFile", S_IRWXA),
            -(Errno::EEXIST as i32)
        );

        // Clean up and finalize
        assert_eq!(cage.unlink_syscall("/symlinkFile"), 0);
        assert_eq!(cage.unlink_syscall("/originalFile"), 0);
        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_open_empty_directory() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);
        let path = "";
        // Check for error when directory is empty
        let result = cage.open_syscall(path, O_CREAT | O_TRUNC | O_RDWR, S_IRWXA);
        if result < 0 {
            let errno_val = get_errno();
            match errno_val {
                libc::ENOENT => assert_eq!(errno_val, libc::ENOENT), // No such file or directory
                libc::EISDIR => assert_eq!(errno_val, libc::EISDIR), // Is a directory
                _ => panic!("Unexpected error code: {}", errno_val),
            }
        } else {
            panic!("Expected failure, but open_syscall succeeded.");
        }
        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_open_nonexisting_parentdirectory_and_file() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);
        let path = "/dir/file";
        // Check for error when neither file nor parent exists and O_CREAT flag is not
        // present
        cage.open_syscall(path, F_GETFD, S_IRWXA);
        let err = get_errno();
        assert_eq!(err, libc::ENOENT, "Expected ENOENT, got {}", err);

        // Check for error when neither file nor parent exists and O_CREAT flag is present
        cage.open_syscall(path, O_CREAT, S_IRWXA);
        let err2 = get_errno();
        assert_eq!(err2, libc::ENOENT, "Expected ENOENT, got {}", err2);
        // Clean up and finalize
        let _ = cage.unlink_syscall(path);
        let _ = cage.rmdir_syscall("/dir");
        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_open_existing_parentdirectory_and_nonexisting_file() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);
        // Create a parent directory
        assert_eq!(cage.mkdir_syscall("/dir", S_IRWXA), 0);
        let path = "/dir/file";

        // Check for error when parent directory exists but file doesn't exist and
        // O_CREAT is not present
        assert_eq!(
            cage.open_syscall(path, O_TRUNC, S_IRWXA),
            -(Errno::ENOENT as i32)
        );

        // Check for error when parent directory exists but file doesn't exist and
        // Filetype Flags contain S_IFCHR flag
        assert_eq!(
            cage.open_syscall(path, 0o20000 | O_CREAT, S_IRWXA),
            -(Errno::EINVAL as i32)
        );

        // Check for error when parent directory exists but file doesn't exist and mode
        // bits are invalid
        let invalid_mode = 0o77777;
        assert_eq!(
            cage.open_syscall(path, O_CREAT, invalid_mode),
            -(Errno::EPERM as i32)
        );
        let _ = cage.unlink_syscall("/dir/file");
        let _ = cage.rmdir_syscall("/dir");

        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_open_existing_file_without_flags() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        // This test is used for validating two scenarios:
        // 1. When the non-existing file is opened using O_CREAT flag, it should open
        //    successfully.
        // 2. When the same existing file is being opened without O_CREAT flag, it
        //    should open successfully.
        let cage = interface::cagetable_getref(1);

        // Open a non-existing file with O_CREAT flag
        // This should create a new file with a valid file descriptor
        let path = "/test";
        let fd = cage.open_syscall(path, O_CREAT | O_RDWR, S_IRWXA);
        assert!(fd > 0);

        // Open the existing file without O_CREAT and O_EXCL
        // The file should open successfully as the two flags are not set while
        // re-opening the file
        let fd2 = cage.open_syscall(path, O_RDONLY, 0);
        assert!(fd2 > 0);

        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_open_existing_file_with_flags() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        // This test is used for validating two scenarios:
        // 1. When the non-existing file is opened using O_CREAT flag, it should open
        //    successfully.
        // 2. When the same existing file is opened using O_CREAT and O_EXCL flags, it
        //    should return an error for file already existing.
        let cage = interface::cagetable_getref(1);

        // Open a non-existing file with O_CREAT flag
        // This should create a new file with a valid file descriptor
        let path = "/test";
        let fd = cage.open_syscall(path, O_CREAT | O_RDWR, S_IRWXA);
        assert!(fd > 0);

        // Open the existing file with O_CREAT and O_EXCL flags
        // The file should not open successfully as the two flags are set while
        // re-opening the file It should return an error for "File already
        // exists"
        assert_eq!(
            cage.open_syscall(path, O_CREAT | O_EXCL | O_RDONLY, S_IRWXA),
            -(Errno::EEXIST as i32)
        );

        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_open_create_new_file_and_check_link_count() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        // Create a new file
        let path = "/newfile.txt";
        let fd = cage.open_syscall(path, O_CREAT | O_RDWR, S_IRWXA);
        assert!(fd > 0);

        // Write a string to the newly opened file of size 12
        assert_eq!(cage.write_syscall(fd, str2cbuf("hello there!"), 12), 12);

        // Get the stat data for the file and check for file attributes
        let mut statdata = StatData::default();
        assert_eq!(cage.stat_syscall(path, &mut statdata), 0);

        // Validate the link count for the new file to be 1
        assert_eq!(statdata.st_nlink, 1);

        // Validate the size of the file to be 12
        assert_eq!(statdata.st_size, 12);

        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_open_existing_file_with_o_trunc_flag() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        // Create a new file
        let path = "/file.txt";
        let fd = cage.open_syscall(path, O_CREAT | O_WRONLY, S_IRWXA);
        assert!(fd > 0);
        // Write a string to the newly opened file of size 12
        assert_eq!(cage.write_syscall(fd, str2cbuf("hello there!"), 12), 12);
        // Get the stat data for the file and check for file attributes
        let mut statdata = StatData::default();
        assert_eq!(cage.stat_syscall(path, &mut statdata), 0);
        // Validate the size of the file to be 12
        assert_eq!(statdata.st_size, 12);

        // Open the same file with O_TRUNC flag
        // Since the file is truncated, the size of the file should be truncated to 0.
        let fd2 = cage.open_syscall(path, O_WRONLY | O_TRUNC, S_IRWXA);
        assert!(fd2 > 0);
        // Get the stat data for the same file and check for file attributes
        assert_eq!(cage.stat_syscall(path, &mut statdata), 0);
        // Validate the size of the file to be 0 as the file is truncated now
        assert_eq!(statdata.st_size, 0);

        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_open_new_file_with_s_ifchar_flag() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        // Create a parent directory
        assert_eq!(cage.mkdir_syscall("/testdir", S_IRWXA), 0);
        let path = "/testdir/file";

        // Attempt to open a file with S_IFCHR flag, which should be invalid for regular
        // files
        assert_eq!(
            cage.open_syscall(path, O_CREAT | 0o20000, S_IRWXA),
            -(Errno::EINVAL as i32)
        );

        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_creat_new_file() {
        // Since this call is almost similar to open_syscall, and we have
        // covered all the possible test scenarios for open_syscall above. So,
        // just testing the basic working flow for the creat_sycall.

        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        // Create a file and validate the size of it.
        let path = "/creatFile";
        let fd = cage.creat_syscall(path, S_IRWXA);
        assert!(fd > 0);

        let mut statdata = StatData::default();

        // The size of the file should be 0
        assert_eq!(cage.stat_syscall(path, &mut statdata), 0);
        assert_eq!(statdata.st_size, 0);

        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_creat_truncate_existing_file() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        let path = "/creatFile";
        // Create a new file
        let fd = cage.creat_syscall(path, S_IRWXA);

        // Write a string to the newly opened file of size 12
        assert_eq!(cage.write_syscall(fd, str2cbuf("hello there!"), 12), 12);

        // Get the stat data for the file and check for file attributes
        let mut statdata = StatData::default();
        assert_eq!(cage.stat_syscall(path, &mut statdata), 0);

        // Validate the size of the file to be 12
        assert_eq!(statdata.st_size, 12);

        // Call the function on the existing file, which should truncate
        // the file size to 0.
        let _fd2 = cage.creat_syscall(path, S_IRWXA);
        assert_eq!(cage.stat_syscall(path, &mut statdata), 0);

        // Validate the size of the file to be 0 now as should be truncated
        assert_eq!(statdata.st_size, 0);

        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_read_write_only_fd() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        // Test to create a file with write only permissions, and check if
        // a valid error is returned when the file is used for reading.
        let fd = cage.open_syscall("/test_file", O_CREAT | O_WRONLY, S_IRWXA);
        let mut read_buf = sizecbuf(5);
        assert_eq!(
            cage.read_syscall(fd, read_buf.as_mut_ptr(), 5),
            -(Errno::EBADF as i32)
        );

        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_read_from_directory() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        // Create a directory and try to read from it.
        // We should expect an error (EISDIR) as reading from a directory is not
        // supported.
        let path = "/test_dir";
        // Clear the directory if it exists
        let _ = cage.rmdir_syscall(path);
        assert_eq!(cage.mkdir_syscall(path, S_IRWXA), 0);
        let fd = cage.open_syscall(path, O_RDONLY, S_IRWXA);

        let mut read_buf = sizecbuf(5);
        assert_eq!(
            cage.read_syscall(fd, read_buf.as_mut_ptr(), 5),
            -(Errno::EISDIR as i32)
        );
        let _ = cage.rmdir_syscall(path);
        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_read_from_epoll() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        // Create an Epoll and try to read from it.
        // We should expect an error (EINVAL) as reading from an Epoll is not supported.
        let epfd = cage.epoll_create_syscall(1);
        assert!(epfd > 0);
        let mut read_buf = sizecbuf(5);
        assert_eq!(
            cage.read_syscall(epfd, read_buf.as_mut_ptr(), 5),
            -(Errno::EINVAL as i32)
        );

        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_read_from_regular_file() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        // This test mainly tests two scenarios for reading from a regular file:
        // * Reading from a file should initially start from 0 position.
        // * Once read, the position of the seek pointer in the file descriptor should
        // increment by the count of bytes read. If the read is performed again, then
        // the position should continue from the point it was previously left.
        let fd = cage.open_syscall("/test_file", O_CREAT | O_TRUNC | O_RDWR, S_IRWXA);
        assert!(fd >= 0);

        // Write sample data to the file.
        assert_eq!(cage.write_syscall(fd, str2cbuf("hello there!"), 12), 12);

        // Set the initial position to 0 in the file descriptor.
        assert_eq!(cage.lseek_syscall(fd, 0, SEEK_SET), 0);

        // Read first 5 bytes from the file, and assert the result.
        let mut read_buf1 = sizecbuf(5);
        assert_eq!(cage.read_syscall(fd, read_buf1.as_mut_ptr(), 5), 5);
        assert_eq!(cbuf2str(&read_buf1), "hello");

        // Read next 7 bytes which should start from the previous position.
        let mut read_buf2 = sizecbuf(7);
        assert_eq!(cage.read_syscall(fd, read_buf2.as_mut_ptr(), 7), 7);
        assert_eq!(cbuf2str(&read_buf2), " there!");

        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_read_from_chardev_file() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        // This test mainly tests the case for reading from a character device type
        // file. In this case, we are trying to read 100 bytes from the
        // "/dev/zero" file, which should return 100 bytes of "0" filled
        // characters.
        let path = "/dev/zero";
        // We are creating /dev/zero manually in this test since we are in the sandbox env.
        // In a real system, /dev/zero typically exists as a special device file.
        // Create a /dev directory if it doesn't exist
        cage.mkdir_syscall("/dev", S_IRWXA);
        if cage.access_syscall(path, F_OK) != 0 {
            let fd = cage.open_syscall(path, O_CREAT | O_TRUNC | O_RDWR, S_IRWXA);
            // Write 100 bytes of 0 to mimic /dev/zero behavior
            let write_data = vec![0u8; 100];
            assert_eq!(
                cage.write_syscall(fd, write_data.as_ptr(), 100),
                100,
                "Failed to write zeros to /dev/zero"
            );
            assert_eq!(cage.close_syscall(fd), 0);
        }
        // Open the test file again for reading
        let fd = cage.open_syscall(path, O_RDWR, S_IRWXA);

        // Verify if the returned count of bytes is 100.
        // Seek to the beginning of the file
        assert_eq!(
            cage.lseek_syscall(fd, 0, libc::SEEK_SET),
            0,
            "Failed to seek to the beginning of /dev/zero"
        );
        // Read 100 bytes from the file
        let mut read_bufzero = sizecbuf(100);
        assert_eq!(cage.read_syscall(fd, read_bufzero.as_mut_ptr(), 100), 100);
        // Verify if the characters present in the buffer are all "0".
        assert_eq!(
            cbuf2str(&read_bufzero),
            std::iter::repeat("\0")
                .take(100)
                .collect::<String>()
                .as_str()
        );
        assert_eq!(cage.close_syscall(fd), 0);
        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_read_from_sockets() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        // This test mainly tests the case for reading data from a pair of Sockets.
        // In this case, we create a socket pair of two sockets, and send data through
        // one socket, and try to read it from the other one using `read_syscall()`.
        let mut socketpair = SockPair::default();

        // Verify if the socketpair is formed successfully.
        assert_eq!(
            Cage::socketpair_syscall(
                &cage.clone(),
                libc::AF_UNIX,
                libc::SOCK_STREAM,
                0,
                &mut socketpair
            ),
            0
        );
        // Verify if the number of bytes sent to socket1 is correct.
        assert_eq!(
            cage.send_syscall(socketpair.sock1, str2cbuf("test"), 4, 0),
            4
        );
        // Verify if the number of bytes received by socket2 is correct.
        let mut buf2 = sizecbuf(4);
        assert_eq!(cage.read_syscall(socketpair.sock2, buf2.as_mut_ptr(), 4), 4);
        // Verify if the data received inside the buffer is correct.
        assert_eq!(cbuf2str(&buf2), "test");
        // Close the sockets
        assert_eq!(cage.close_syscall(socketpair.sock1), 0);
        assert_eq!(cage.close_syscall(socketpair.sock2), 0);
        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_read_from_pipe_blocking_mode() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        // This test mainly tests the case of reading data from the pipe.
        // We create two pipes, i.e., Read and Write and validate if the data
        // received is correct or not.

        // Create a pipe of read and write file descriptors.
        let mut pipe_fds = PipeArray::default();
        assert_eq!(cage.pipe_syscall(&mut pipe_fds), 0);
        let read_fd = pipe_fds.readfd;
        let write_fd = pipe_fds.writefd;

        let write_data = "Testing";
        let mut buf = sizecbuf(7);

        // Write data to the pipe
        assert_eq!(
            cage.write_syscall(write_fd, write_data.as_ptr(), write_data.len()),
            write_data.len() as i32
        );

        // Read the data from the pipe and verify its count.
        assert_eq!(
            cage.read_syscall(read_fd, buf.as_mut_ptr(), buf.len()),
            write_data.len() as i32
        );
        // Verify if the data returned in the pipe buffer is correct.
        assert_eq!(cbuf2str(&buf), write_data);

        // Close the file descriptors
        assert_eq!(cage.close_syscall(read_fd), 0);
        assert_eq!(cage.close_syscall(write_fd), 0);
        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_read_from_pipe_nonblocking_mode() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        // This test mainly tests the case of reading data from the pipe, but in
        // non-blocking mode. We create two pipes, i.e., Read and Write and
        // validate if the data received is correct or not.

        // Create a pipe of read and write file descriptors.
        let mut pipe_fds = PipeArray::default();
        assert_eq!(cage.pipe_syscall(&mut pipe_fds), 0);
        let read_fd = pipe_fds.readfd;
        let write_fd = pipe_fds.writefd;

        let write_data = "Testing";
        let mut buf = sizecbuf(7);

        // Set pipe to non-blocking mode
        assert_eq!(cage.fcntl_syscall(read_fd, F_SETFL, O_NONBLOCK), 0);

        // Read from the pipe (should return EAGAIN as there's no data yet)
        assert_eq!(
            cage.read_syscall(read_fd, buf.as_mut_ptr(), buf.len()),
            -(Errno::EAGAIN as i32)
        );

        // Write data to the pipe
        assert_eq!(
            cage.write_syscall(write_fd, write_data.as_ptr(), write_data.len()),
            write_data.len() as i32
        );

        // Read the data from the pipe and verify its count.
        assert_eq!(
            cage.read_syscall(read_fd, buf.as_mut_ptr(), buf.len()),
            write_data.len() as i32
        );
        // Verify if the data returned in the pipe buffer is correct.
        assert_eq!(cbuf2str(&buf), write_data);

        // Close the file descriptors
        assert_eq!(cage.close_syscall(read_fd), 0);
        assert_eq!(cage.close_syscall(write_fd), 0);
        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_pread_write_only_fd() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        // Test to create a file with write only permissions, and check if
        // a valid error is returned when the file is used for reading.
        let fd = cage.open_syscall("/test_file", O_CREAT | O_WRONLY, S_IRWXA);
        let mut read_buf = sizecbuf(5);
        assert_eq!(
            cage.pread_syscall(fd, read_buf.as_mut_ptr(), 5, 0),
            -(Errno::EBADF as i32)
        );

        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_pread_from_file() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        // This test mainly tests two scenarios for reading from a file using
        // `pread_syscall()`.
        // * Reading from a file from the starting position offset(0).
        // * Reading from a file from a random position offset.
        let fd = cage.open_syscall("/test_file", O_CREAT | O_TRUNC | O_RDWR, S_IRWXA);
        assert!(fd >= 0);

        // Write sample data to the file.
        assert_eq!(cage.write_syscall(fd, str2cbuf("hello there!"), 12), 12);

        // Set the initial position to 0 in the file descriptor.
        assert_eq!(cage.lseek_syscall(fd, 0, SEEK_SET), 0);

        // Read first 5 bytes from the file, and assert the result.
        let mut read_buf1 = sizecbuf(5);
        assert_eq!(cage.pread_syscall(fd, read_buf1.as_mut_ptr(), 5, 0), 5);
        assert_eq!(cbuf2str(&read_buf1), "hello");

        // Read 5 bytes, but from the 6th position offset of the file.
        let mut read_buf2 = sizecbuf(5);
        assert_eq!(cage.pread_syscall(fd, read_buf2.as_mut_ptr(), 5, 6), 5);
        assert_eq!(cbuf2str(&read_buf2), "there");

        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_pread_from_directory() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);
        let mut buf = sizecbuf(5);

        // Test for invalid directory should fail
        let path = "/test_dir";
        // Remove the directory if it exists to ensure a clean test environment
        let _ = cage.rmdir_syscall(path);
        assert_eq!(cage.mkdir_syscall(path, S_IRWXA), 0);
        // Open the directory with O_RDONLY (appropriate for directories)
        let fd = cage.open_syscall(path, O_RDONLY, S_IRWXA);
        assert!(fd >= 0);
        assert_eq!(
            cage.pread_syscall(fd, buf.as_mut_ptr(), buf.len(), 0),
            -(Errno::EISDIR as i32)
        );
        // Clean up the directory for clean environment
        assert_eq!(cage.rmdir_syscall(path), 0);
        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_pread_invalid_types() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);
        let mut buf = sizecbuf(5);

        // Test for invalid pipe
        // Try reading the data from the pipe and check for error.
        let mut pipe_fds = PipeArray::default();
        assert_eq!(cage.pipe_syscall(&mut pipe_fds), 0);
        let read_fd = pipe_fds.readfd;
        assert_eq!(
            cage.pread_syscall(read_fd, buf.as_mut_ptr(), buf.len(), 0),
            -(Errno::ESPIPE as i32)
        );

        // Test for invalid sockets
        // Try reading the data from the socket and check for error.
        let mut socketpair = SockPair::default();
        assert_eq!(
            Cage::socketpair_syscall(
                &cage.clone(),
                libc::AF_UNIX,
                libc::SOCK_STREAM,
                0,
                &mut socketpair
            ),
            0
        );
        assert_eq!(
            cage.pread_syscall(socketpair.sock2, buf.as_mut_ptr(), 4, 0),
            -(Errno::ESPIPE as i32)
        );

        // Test for invalid epoll
        // Try reading the data from the epoll and check for error.
        let epfd = cage.epoll_create_syscall(1);
        assert_eq!(
            cage.pread_syscall(epfd, buf.as_mut_ptr(), 5, 0),
            -(Errno::ESPIPE as i32)
        );
        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_write_read_only_fd() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        // Test to write to a file with read only permissions, and check if
        // a valid error is returned when the file is used for writing.
        let fd = cage.open_syscall("/test_file", O_CREAT | O_RDONLY, S_IRWXA);
        assert!(fd >= 0);

        let write_data = "hello";
        assert_eq!(
            cage.write_syscall(fd, write_data.as_ptr(), write_data.len()),
            -(Errno::EBADF as i32)
        );

        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_write_to_directory() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        // Create a directory and try to write to it.
        // We should expect an error (EISDIR) as writing to a directory is not
        // supported.
        let path = "/test_dir";
        // Remove the directory if it exists to ensure a clean test environment
        let _ = cage.rmdir_syscall(path);
        // Create the directory
        assert_eq!(cage.mkdir_syscall(path, S_IRWXA), 0);
        // Attempt to open the directory with O_WRONLY, expecting EISDIR
        let fd_wr = cage.open_syscall(path, O_WRONLY, S_IRWXA);
        assert_eq!(fd_wr, -(Errno::EISDIR as i32));

        // Open the directory with O_RDONLY to get a valid file descriptor
        let fd_rd = cage.open_syscall(path, O_RDONLY, S_IRWXA);
        assert!(
            fd_rd >= 0,
            "Failed to open directory with O_RDONLY, got error code: {}",
            fd_rd
        );
        let write_data = "hello";
        let write_result = cage.write_syscall(fd_rd, write_data.as_ptr(), write_data.len());
        assert_eq!(write_result, -(Errno::EBADF as i32));

        // Clean up
        assert_eq!(cage.close_syscall(fd_rd), 0);
        assert_eq!(cage.rmdir_syscall(path), 0);
        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_write_to_epoll() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        // Create an Epoll and try to write to it.
        // We should expect an error (EINVAL) as writing to an Epoll is not supported.
        let epfd = cage.epoll_create_syscall(1);
        assert!(epfd > 0);
        let write_data = "hello";
        assert_eq!(
            cage.write_syscall(epfd, write_data.as_ptr(), write_data.len()),
            -(Errno::EINVAL as i32)
        );

        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_write_to_regular_file() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        // This test mainly tests writing to a regular file.
        // * Writing data to a file should start from position 0.
        // * Once written, the position of the seek pointer in the file descriptor
        // should increment by the count of bytes written. If write is performed again,
        // then the position should continue from the point it was previously left.
        let fd = cage.open_syscall("/test_file", O_CREAT | O_TRUNC | O_RDWR, S_IRWXA);
        assert!(fd >= 0);

        let mut statdata = StatData::default();

        // Write sample data to the file, and verify the number of bytes returned
        let write_data1 = "hello";
        assert_eq!(
            cage.write_syscall(fd, write_data1.as_ptr(), write_data1.len()),
            5
        );

        // Verify the size of the file
        assert_eq!(cage.fstat_syscall(fd, &mut statdata), 0);
        assert_eq!(statdata.st_size, 5);

        // Write additional data to the file.
        let write_data2 = " there!";
        assert_eq!(
            cage.write_syscall(fd, write_data2.as_ptr(), write_data2.len()),
            7
        );

        // Verify the updated size of the file
        assert_eq!(cage.fstat_syscall(fd, &mut statdata), 0);
        assert_eq!(statdata.st_size, 12);

        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_write_to_chardev_file() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        // This test mainly tests the case for writing to a character device type
        // file. In this case, we are trying to write 100 bytes to the
        // "/dev/null" file, which should succeed without doing anything.
        let path = "/dev/null";
        let fd = cage.open_syscall(path, O_RDWR, S_IRWXA);

        // Verify if the returned count of bytes is 100.
        let write_data = "0".repeat(100);
        assert_eq!(cage.write_syscall(fd, write_data.as_ptr(), 100), 100);

        assert_eq!(cage.close_syscall(fd), 0);
        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_write_to_sockets() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        // This test mainly tests the case for writing data to a pair of Sockets.
        // In this case, we create a socket pair of two sockets, and send data through
        // one socket, and try to read it from the other one.
        let mut socketpair = SockPair::default();

        // Verify if the socketpair is formed successfully.
        assert_eq!(
            Cage::socketpair_syscall(
                &cage.clone(),
                libc::AF_UNIX,
                libc::SOCK_STREAM,
                0,
                &mut socketpair
            ),
            0
        );
        // Verify if the number of bytes sent to socket1 is correct.
        let write_data = "test";
        assert_eq!(
            cage.write_syscall(socketpair.sock1, write_data.as_ptr(), 4),
            4
        );

        // Verify if the number of bytes received by socket2 is correct.
        let mut buf2 = sizecbuf(4);
        assert_eq!(cage.read_syscall(socketpair.sock2, buf2.as_mut_ptr(), 4), 4);
        // Verify if the data received inside the buffer is correct.
        assert_eq!(cbuf2str(&buf2), "test");

        // Close the sockets
        assert_eq!(cage.close_syscall(socketpair.sock1), 0);
        assert_eq!(cage.close_syscall(socketpair.sock2), 0);
        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_pwrite_read_only_fd() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        // Test to write to a file with read only permissions, and check if
        // a valid error is returned when the file is used for writing.
        let fd = cage.open_syscall("/test_file", O_CREAT | O_RDONLY, S_IRWXA);
        assert!(fd >= 0);

        let write_data = "hello";
        assert_eq!(
            cage.pwrite_syscall(fd, write_data.as_ptr(), write_data.len(), 0),
            -(Errno::EBADF as i32)
        );

        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_pwrite_to_file() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        // This test mainly tests two scenarios for writing to a file using
        // `pwrite_syscall()`.
        // * Writing to a file from the starting position offset(0).
        // * Writing to a file from a random position offset, which should
        // pad the file with additional "\0" bytes.
        let fd = cage.open_syscall("/test_file", O_CREAT | O_TRUNC | O_RDWR, S_IRWXA);
        assert!(fd >= 0);

        // Write sample data to the file and verify the number of bytes returned.
        let write_data1 = "hello";
        assert_eq!(cage.pwrite_syscall(fd, write_data1.as_ptr(), 5, 0), 5);

        // Write additional data to the file starting from the 6th position offset.
        let write_data2 = "there!";
        assert_eq!(cage.pwrite_syscall(fd, write_data2.as_ptr(), 6, 6), 6);

        // Read back the data to verify, but since we are changing the offset to
        // a larger number than the file size, it should pad the file with "\0" values.
        // Verify if the file contains the paded bytes as well.
        let mut read_buf = sizecbuf(12);
        assert_eq!(cage.pread_syscall(fd, read_buf.as_mut_ptr(), 12, 0), 12);
        assert_eq!(cbuf2str(&read_buf), "hello\0there!");

        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_pwrite_to_directory() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        // Create a directory and try to write to it.
        // We should expect an error (EISDIR) as writing to a directory is not
        // supported.
        let path = "/test_dir";
        // Remove the directory if it exists to ensure a clean test environment
        let _ = cage.rmdir_syscall(path);
        assert_eq!(cage.mkdir_syscall(path, S_IRWXA), 0);
        // Open the directory with O_RDONLY
        let fd = cage.open_syscall(path, O_RDONLY, S_IRWXA);
        assert!(fd >= 0, "Failed to open directory: invalid file descriptor");

        let write_data = "hello";
        // Attempt to pwrite to the directory, expecting EBADF.
        let result = cage.pwrite_syscall(fd, write_data.as_ptr(), write_data.len(), 0);
        // Expect EBADF (Bad file descriptor) as directories cannot be written to.
        assert_eq!(
            result,
            -(Errno::EBADF as i32),
            "Expected EBADF error when attempting to write to a directory"
        );

        // Clean up
        assert_eq!(cage.rmdir_syscall(path), 0);
        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_pwrite_invalid_types() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        // Test for invalid pipe
        // Try writing the data to the pipe and check for error.
        let mut pipe_fds = PipeArray::default();
        assert_eq!(cage.pipe_syscall(&mut pipe_fds), 0);
        let write_fd = pipe_fds.writefd;
        let write_data = "hello";
        assert_eq!(
            cage.pwrite_syscall(write_fd, write_data.as_ptr(), write_data.len(), 0),
            -(Errno::ESPIPE as i32)
        );

        // Test for invalid sockets
        // Try writing the data to the socket and check for error.
        let mut socketpair = SockPair::default();
        assert_eq!(
            Cage::socketpair_syscall(
                &cage.clone(),
                libc::AF_UNIX,
                libc::SOCK_STREAM,
                0,
                &mut socketpair
            ),
            0
        );
        assert_eq!(
            cage.pwrite_syscall(socketpair.sock2, write_data.as_ptr(), 4, 0),
            -(Errno::ESPIPE as i32)
        );

        // Test for invalid epoll
        // Try writing the data to the epoll and check for error.
        let epfd = cage.epoll_create_syscall(1);
        assert_eq!(
            cage.pwrite_syscall(epfd, write_data.as_ptr(), 5, 0),
            -(Errno::ESPIPE as i32)
        );

        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_pwrite_to_chardev_file() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        // This test mainly tests the case for writing to a character device type
        // file. In this case, we are trying to write 100 bytes to the
        // "/dev/null" file, which should succeed without doing anything.
        let path = "/dev/null";
        // We are creating /dev/null manually in this test since we are in the sandbox env.
        // In a real system, /dev/null typically exists as a special device file.
        // Make the folder if it doesn't exist
        let _ = cage.mkdir_syscall("/dev", S_IRWXA);
        let fd = cage.open_syscall(path, O_RDWR | O_CREAT, S_IRWXA);

        // Verify if the returned count of bytes is 100.
        let write_data = "0".repeat(100);
        assert_eq!(cage.pwrite_syscall(fd, write_data.as_ptr(), 100, 0), 100);

        assert_eq!(cage.close_syscall(fd), 0);
        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_fs_shmget_syscall() {
        // acquire locks and start env cleanup
        let _thelock = setup::lock_and_init();
        let cage = interface::cagetable_getref(1);

        let key = 33123;
        // Get shmid of a memory segment / create a new one if it doesn't exist
        let shmid = cage.shmget_syscall(33123, 1024, IPC_CREAT);
        assert_eq!(shmid, 4);

        // Check error upon asking for a valid key and passing the IPC_CREAT and
        // IPC_EXCL flag
        assert_eq!(
            cage.shmget_syscall(key, 1024, IPC_CREAT | IPC_EXCL),
            -(Errno::EEXIST as i32)
        );

        // Check error when passing IPC_CREAT flag as the key
        assert_eq!(
            cage.shmget_syscall(IPC_PRIVATE, 1024, IPC_PRIVATE),
            -(Errno::ENOENT as i32)
        );

        // Check if the function returns a correct shmid upon asking with a key that we
        // know exists
        assert_eq!(cage.shmget_syscall(key, 1024, 0666), shmid);

        // Check if the function returns the correct error when we don't pass IPC_CREAT
        // for a key that doesn't exist
        assert_eq!(
            cage.shmget_syscall(123456, 1024, 0),
            -(Errno::ENOENT as i32)
        );

        // Check if the size error is returned correctly
        assert_eq!(
            cage.shmget_syscall(123456, (SHMMAX + 10) as usize, IPC_CREAT),
            -(Errno::EINVAL as i32)
        );
        assert_eq!(
            cage.shmget_syscall(123456, 0 as usize, IPC_CREAT),
            -(Errno::EINVAL as i32)
        );

        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_lseek_on_file() {
        // acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        // Test to create a file and check if seeking to a new location is possible.
        let fd = cage.open_syscall("/test_file", O_CREAT | O_WRONLY, S_IRWXA);
        assert!(fd >= 0);

        // Attempt to seek within the file and check if it succeeds
        assert_eq!(cage.lseek_syscall(fd, 10, SEEK_SET), 10);

        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_lseek_on_directory() {
        // acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        // Create a directory and try to seek within it.
        let path = "/test_dir";
        assert_eq!(cage.mkdir_syscall(path, S_IRWXA), 0);
        let fd = cage.open_syscall(path, O_RDONLY, S_IRWXA);
        assert!(fd >= 0);

        // Attempt to seek within the directory and check if it succeeds
        assert_eq!(cage.lseek_syscall(fd, 1, SEEK_SET), 1);
        // Clean up the directory for clean environment
        assert_eq!(cage.rmdir_syscall(path), 0);
        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_lseek_invalid_whence() {
        // acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        // Test to create a file and check for invalid `whence` value
        let fd = cage.open_syscall("/test_file", O_CREAT | O_RDWR, S_IRWXA);
        assert!(fd >= 0);

        // Attempt to seek with an invalid `whence` value and check if it returns an
        // error
        assert_eq!(
            cage.lseek_syscall(fd, 10, 999), // Invalid whence value
            -(Errno::EINVAL as i32)
        );

        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_lseek_beyond_file_size() {
        // acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);
        // Remove the file if it exists to ensure a clean test environment
        let _ = cage.unlink_syscall("/test_file");
        // Test to create a file and seek beyond its size
        let fd = cage.open_syscall("/test_file", O_CREAT | O_RDWR, S_IRWXA);
        assert!(fd >= 0);

        // Write sample data to the file.
        assert_eq!(cage.write_syscall(fd, str2cbuf("hello"), 5), 5);

        // Seek beyond the end of the file and verify if it succeeds
        assert_eq!(
            cage.lseek_syscall(fd, 10, SEEK_END),
            15 // 5 (file size) + 10 (offset)
        );
        // Clean up the file for clean environment
        assert_eq!(cage.unlink_syscall("/test_file"), 0);
        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_lseek_before_start_of_file() {
        // acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        // Test to create a file and attempt to seek before the start of the file
        let fd = cage.open_syscall("/test_file", O_CREAT | O_RDWR, S_IRWXA);
        assert!(fd >= 0);

        // Attempt to seek to a negative offset and check if it returns an error
        // using "SEEK_SET" whence, where we are explicitly setting the file
        // offset to -10 value.
        assert_eq!(
            cage.lseek_syscall(fd, -10, SEEK_SET),
            -(Errno::EINVAL as i32)
        );

        // Attempt to seek to a negative offset and check if it returns an error
        // using "SEEK_CUR" whence, where current position of the file is 0,
        // as it's empty initially, and we are adding -10 to the offset.
        assert_eq!(
            cage.lseek_syscall(fd, -10, SEEK_CUR),
            -(Errno::EINVAL as i32)
        );

        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_lseek_on_pipe() {
        // acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        // Create a pipe and attempt to seek within it
        let mut pipe_fds = PipeArray::default();
        assert_eq!(cage.pipe_syscall(&mut pipe_fds), 0);
        let read_fd = pipe_fds.readfd;

        // Attempt to seek within the pipe and check if it returns an error
        assert_eq!(
            cage.lseek_syscall(read_fd, 10, SEEK_SET),
            -(Errno::ESPIPE as i32)
        );

        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_lseek_on_chardev() {
        // acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        // Attempt to seek within a character device file
        let path = "/dev/null";
        let fd = cage.open_syscall(path, O_RDWR, S_IRWXA);

        // Seek within the character device and check if it returns 0 (no operation)
        assert_eq!(cage.lseek_syscall(fd, 10, SEEK_SET), 0);

        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_lseek_on_epoll() {
        // acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        // Create an Epoll and try to seek from it.
        let epfd = cage.epoll_create_syscall(1);
        assert!(epfd > 0);

        // Attempt to seek from the epoll and check if it returns an error
        let lseek_result = unsafe { libc::lseek(epfd, 10, libc::SEEK_SET) };
        assert_eq!(lseek_result, -1);
        // If lseek failed, check the errno
        let errno = unsafe { *libc::__errno_location() };
        assert_eq!(errno, libc::ESPIPE, "Expected ESPIPE error, got: {}", errno);
        // Exit and finalize
        let exit_status = cage.exit_syscall(libc::EXIT_SUCCESS);
        assert_eq!(exit_status, libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_close_regular_file() {
        // acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        // Create and open a regular file, then close it.
        let fd = cage.open_syscall("/test_file", O_CREAT | O_RDWR, S_IRWXA);
        assert!(fd >= 0);

        // Write sample data to the file.
        assert_eq!(cage.write_syscall(fd, str2cbuf("hello"), 5), 5);

        // Close the file descriptor, which should succeed.
        assert_eq!(cage.close_syscall(fd), 0);

        // Attempt to close the file descriptor again to ensure it's already closed.
        // Expect an error for "Invalid File Descriptor".
        assert_eq!(cage.close_syscall(fd), -(Errno::EBADF as i32));

        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_close_directory() {
        // acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        // Create a directory and open it.
        let path = "/test_dir";
        assert_eq!(cage.mkdir_syscall(path, S_IRWXA), 0);
        let fd = cage.open_syscall(path, O_RDONLY, S_IRWXA);
        assert!(fd >= 0);

        // Close the directory file descriptor, which should succeed.
        assert_eq!(cage.close_syscall(fd), 0);

        // Attempt to close the file descriptor again to ensure it's already closed.
        // Expect an error for "Invalid File Descriptor".
        assert_eq!(cage.close_syscall(fd), -(Errno::EBADF as i32));
        // Remove the directory to clean up the environment
        assert_eq!(cage.rmdir_syscall(path), 0);
        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_close_socket() {
        // acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        // Create a socket pair.
        let mut socketpair = SockPair::default();
        assert_eq!(
            Cage::socketpair_syscall(
                &cage.clone(),
                libc::AF_UNIX,
                libc::SOCK_STREAM,
                0,
                &mut socketpair
            ),
            0
        );

        // Close both the socket file descriptors, which should succeed.
        assert_eq!(cage.close_syscall(socketpair.sock1), 0);
        assert_eq!(cage.close_syscall(socketpair.sock2), 0);

        // Attempt to close the file descriptors again to ensure they are already
        // closed. Expect an error for "Invalid File Descriptor".
        assert_eq!(cage.close_syscall(socketpair.sock1), -(Errno::EBADF as i32));
        assert_eq!(cage.close_syscall(socketpair.sock2), -(Errno::EBADF as i32));

        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_close_pipe() {
        // acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        // Create a pipe.
        let mut pipe_fds = PipeArray::default();
        assert_eq!(cage.pipe_syscall(&mut pipe_fds), 0);
        let read_fd = pipe_fds.readfd;
        let write_fd = pipe_fds.writefd;

        // Write data to the pipe
        let write_data = "Testing";
        assert_eq!(
            cage.write_syscall(write_fd, write_data.as_ptr(), write_data.len()),
            write_data.len() as i32
        );

        // Read the data from the pipe.
        let mut buf = sizecbuf(7);
        assert_eq!(
            cage.read_syscall(read_fd, buf.as_mut_ptr(), buf.len()),
            write_data.len() as i32
        );
        assert_eq!(cbuf2str(&buf), write_data);

        // Close the pipe file descriptors, which should succeed.
        assert_eq!(cage.close_syscall(read_fd), 0);
        assert_eq!(cage.close_syscall(write_fd), 0);

        // Attempt to close the file descriptor again to ensure they are already closed.
        // Expect an error for "Invalid File Descriptor".
        assert_eq!(cage.close_syscall(read_fd), -(Errno::EBADF as i32));
        assert_eq!(cage.close_syscall(write_fd), -(Errno::EBADF as i32));

        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_fs_close_chardev() {
        // acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        // Ideally, we should have a character device file in the system
        // and use that instead of creating a new file. But this is
        // an sandboxed environment, so we need to create a file.
        // Open a character device file.
        let fd = cage.open_syscall("/dev/zero", O_RDWR | O_CREAT, S_IRWXA);
        assert!(fd >= 0);

        // Close the character device file descriptor, which should succeed.
        assert_eq!(cage.close_syscall(fd), 0);

        // Attempt to close the file descriptor again to ensure it's already closed.
        // Expect an error for "Invalid File Descriptor".
        assert_eq!(cage.close_syscall(fd), -(Errno::EBADF as i32));
        // Remove the file to clean up the environment
        let _ = cage.unlink_syscall("/dev/zero");
        assert_eq!(cage.exit_syscall(libc::EXIT_SUCCESS), libc::EXIT_SUCCESS);
        lindrustfinalize();
    }

    // #[test]
    // pub fn ut_lind_fs_stat_syscall_tests() {
    //     // acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
    //     // and also performs clean env setup
    //     let _thelock = setup::lock_and_init();

    //     let cage = interface::cagetable_getref(1);
    //     let mut statdata = StatData::default();

    //     // test out whether an error is output for a non existent file path
    //     // (ENOENT[-2])
    //     assert_eq!(
    //         cage.stat_syscall("non_existent_file_path", &mut statdata),
    //         syscall_error(Errno::ENOENT, "stat", "test_failure")
    //     );

    //     // setting up directory inode object '/tmp' for testing stat_syscall with a
    //     // directory
    //     let dir_path = "/tmp"; // since setup already initializes tmp, assuming it is there
    //     assert_eq!(cage.stat_syscall(dir_path, &mut statdata), 0);

    //     // setting up generic inode object "/tmp/generic" for testing stat_syscall with
    //     // a generic file
    //     let generic_path = "/tmp/generic";
    //     let creat_fd = cage.creat_syscall(generic_path, S_IRWXA);
    //     assert!(creat_fd > 0);
    //     assert_eq!(cage.stat_syscall(generic_path, &mut statdata), 0);

    //     // setting up character device inode object "/chardev" for testing stat_syscall
    //     // with a character device
    //     let dev = makedev(&DevNo { major: 1, minor: 3 });
    //     let chardev_path = "/chardev";
    //     assert_eq!(
    //         cage.mknod_syscall(chardev_path, S_IRWXA | S_IFCHR as u32, dev),
    //         0
    //     );
    //     assert_eq!(cage.stat_syscall(chardev_path, &mut statdata), 0);

    //     // setting up socket inode object with path "/socket.sock"  for testing
    //     // stat_syscall with a socket
    //     let socketfile_path = "/socket.sock";
    //     let socketfd = cage.socket_syscall(AF_UNIX, SOCK_STREAM, 0);
    //     assert!(socketfd > 0);
    //     let sockaddr = interface::new_sockaddr_unix(AF_UNIX as u16, socketfile_path.as_bytes());
    //     let socket = GenSockaddr::Unix(sockaddr);
    //     assert_eq!(cage.bind_syscall(socketfd, &socket), 0);

    //     // stat_syscall test here
    //     assert_eq!(cage.stat_syscall(socketfile_path, &mut statdata), 0);

    //     // socket teardown
    //     assert_eq!(cage.close_syscall(socketfd), 0);
    //     cage.unlink_syscall(socketfile_path);

    //     lindrustfinalize();
    //     return;
    // }

    // #[test]
    // pub fn ut_lind_fs_fstat_syscall_tests() {
    //     //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
    //     // and also performs clean env setup
    //     let _thelock = setup::lock_and_init();

    //     let cage = interface::cagetable_getref(1);

    //     let mut statdata = StatData::default();

    //     // test out whether an error is output for a non existent fd (1000)
    //     // (ENOENT[-2])
    //     let non_existent_fd = 1000;
    //     assert_eq!(cage.fstat_syscall(non_existent_fd, &mut statdata), -9);

    //     // setting up directory inode object '/tmp' for testing fstat_syscall with a
    //     // directory
    //     let dir_path = "/tmp"; // since setup already initializes tmp, assuming it is there
    //     let dir_fd = cage.open_syscall(dir_path, O_RDONLY | O_DIRECTORY, S_IRWXA);
    //     assert!(dir_fd > 0);
    //     assert_eq!(cage.fstat_syscall(dir_fd, &mut statdata), 0);
    //     assert_eq!(cage.close_syscall(dir_fd), 0);

    //     // setting up generic inode object "/tmp/generic" for testing fstat_syscall with
    //     // a generic file
    //     let generic_path = "/tmp/generic";
    //     let creat_fd = cage.creat_syscall(generic_path, S_IRWXA);
    //     assert!(creat_fd > 0);
    //     assert_eq!(cage.fstat_syscall(creat_fd, &mut statdata), 0);

    //     // setting up character device inode object "/chardev" for testing fstat_syscall
    //     // with a character device
    //     let dev = makedev(&DevNo { major: 1, minor: 3 });
    //     let chardev_path = "/chardev";
    //     assert_eq!(
    //         cage.mknod_syscall(chardev_path, S_IRWXA | S_IFCHR as u32, dev),
    //         0
    //     );
    //     let chardev_fd = cage.open_syscall(chardev_path, O_RDONLY, S_IRWXA);
    //     assert!(chardev_fd > 0);
    //     assert_eq!(cage.fstat_syscall(chardev_fd, &mut statdata), 0);
    //     assert_eq!(cage.close_syscall(chardev_fd), 0);

    //     // setting up socket inode object with path "/socket.sock" for testing
    //     // fstat_syscall with a socket
    //     let socketfile_path = "/socket.sock";

    //     let socketfd = cage.socket_syscall(libc::AF_UNIX, libc::SOCK_STREAM, 0);
    //     assert!(socketfd > 0);

    //     let sockaddr = interface::new_sockaddr_unix(libc::AF_UNIX as u16, socketfile_path.as_bytes());
    //     let socket = GenSockaddr::Unix(sockaddr);
    //     assert_eq!(cage.bind_syscall(socketfd, &socket), 0);

    //     // Errno::EOPNOTSUPP : -95
    //     assert_eq!(cage.fstat_syscall(socketfd, &mut statdata), -95);

    //     // Clean up
    //     assert_eq!(cage.close_syscall(socketfd), 0);

    //     cage.unlink_syscall(socketfile_path);

    //     lindrustfinalize();
    //     return;
    // }

    // #[test]
    // pub fn ut_lind_fs_statfs_syscall_tests() {
    //     // acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
    //     // and also performs clean env setup
    //     let _thelock = setup::lock_and_init();

    //     let cage = interface::cagetable_getref(1);
    //     let mut fsdata = FSData::default();

    //     // test out whether an error is output for a non existent file path
    //     // (ENOENT[-2])
    //     assert_eq!(
    //         cage.statfs_syscall("non_existent_file_path", &mut fsdata),
    //         syscall_error(Errno::ENOENT, "stat", "test_failure")
    //     );

    //     // setting up inode object "/tmp/generic" for testing statfs_syscall
    //     let generic_path = "/tmp/generic";
    //     let creat_fd = cage.creat_syscall(generic_path, S_IRWXA);
    //     assert!(creat_fd > 0);
    //     assert_eq!(cage.statfs_syscall(generic_path, &mut fsdata), 0);

    //     lindrustfinalize();
    //     return;
    // }

    // #[test]
    // pub fn ut_lind_fs_fstatfs_syscall_tests() {
    //     //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
    //     // and also performs clean env setup
    //     let _thelock = setup::lock_and_init();

    //     let cage = interface::cagetable_getref(1);

    //     let mut fsdata = FSData::default();

    //     // test out whether an error is output for a non existent fd (1000)
    //     // (ENOENT[-2])
    //     let non_existent_fd = 1000;
    //     assert_eq!(
    //         cage.fstatfs_syscall(non_existent_fd, &mut fsdata),
    //         syscall_error(Errno::EBADF, "stat", "test_failure")
    //     );

    //     // setting up generic inode object "/tmp/generic" for testing fstat_syscall with
    //     // a generic file
    //     let generic_path = "/tmp/generic";
    //     let creat_fd = cage.creat_syscall(generic_path, S_IRWXA);
    //     assert!(creat_fd > 0);
    //     assert_eq!(cage.fstatfs_syscall(creat_fd, &mut fsdata), 0);

    //     // setting up socket inode object with path "/socket.sock" for testing
    //     // fstat_syscall with a socket
    //     let socketfile_path = "/socket.sock";

    //     let socketfd = cage.socket_syscall(libc::AF_UNIX, libc::SOCK_STREAM, 0);
    //     assert!(socketfd > 0);

    //     let sockaddr = interface::new_sockaddr_unix(libc::AF_UNIX as u16, socketfile_path.as_bytes());
    //     let socket = GenSockaddr::Unix(sockaddr);
    //     assert_eq!(cage.bind_syscall(socketfd, &socket), 0);

    //     // Errno::EBADF : -9
    //     assert_eq!(
    //         cage.fstatfs_syscall(socketfd, &mut fsdata),
    //         syscall_error(Errno::EBADF, "stat", "test_failure")
    //     );

    //     // Clean up
    //     assert_eq!(cage.close_syscall(socketfd), 0);

    //     cage.unlink_syscall(socketfile_path);

    //     lindrustfinalize();
    //     return;
    // }
}
