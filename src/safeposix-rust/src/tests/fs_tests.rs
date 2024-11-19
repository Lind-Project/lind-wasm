#[allow(unused_parens)]
#[cfg(test)]
pub mod fs_tests {
    use super::super::*;
    use crate::interface;
    use crate::safeposix::syscalls::fs_calls::*;
    use crate::safeposix::{cage::*, dispatcher::*, filesystem};
    use libc::c_void;
    use std::fs::OpenOptions;
    use std::os::unix::fs::PermissionsExt;

    pub fn test_fs() {
        ut_lind_fs_simple(); // has to go first, else the data files created screw with link count test

        ut_lind_fs_broken_close();
        ut_lind_fs_chmod();
        ut_lind_fs_fchmod();
        ut_lind_fs_dir_chdir();
        ut_lind_fs_dir_mode();
        ut_lind_fs_dir_multiple();
        ut_lind_fs_dup();
        ut_lind_fs_dup2();
        ut_lind_fs_fcntl_valid_args();
        ut_lind_fs_fcntl_invalid_args();
        ut_lind_fs_fcntl_dup();
        ut_lind_fs_ioctl_valid_args();
        ut_lind_fs_ioctl_invalid_args();
        ut_lind_fs_fdflags();
        ut_lind_fs_file_link_unlink();
        ut_lind_fs_file_lseek_past_end();
        ut_lind_fs_fstat_complex();
        ut_lind_fs_getuid();
        ut_lind_fs_load_fs();
        ut_lind_fs_mknod();
        ut_lind_fs_multiple_open();
        ut_lind_fs_rename();
        ut_lind_fs_rmdir();
        ut_lind_fs_stat_file_complex();
        ut_lind_fs_stat_file_mode();
        ut_lind_fs_statfs();
        ut_lind_fs_fstatfs();
        ut_lind_fs_ftruncate();
        ut_lind_fs_truncate();
        ut_lind_fs_getdents();
        ut_lind_fs_dir_chdir_getcwd();
        rdwrtest();
        prdwrtest();
        chardevtest();
        ut_lind_fs_exec_cloexec();
        ut_lind_fs_shm();
        ut_lind_fs_getpid_getppid();
        ut_lind_fs_sem_fork();
        ut_lind_fs_sem_trytimed();
        ut_lind_fs_sem_test();
        ut_lind_fs_tmp_file_test();

        //mkdir_syscall_tests
        ut_lind_fs_mkdir_empty_directory();
        ut_lind_fs_mkdir_nonexisting_directory();
        ut_lind_fs_mkdir_existing_directory();
        ut_lind_fs_mkdir_invalid_modebits();
        ut_lind_fs_mkdir_success();
        ut_lind_fs_mkdir_using_symlink();

        //open_syscall_tests
        ut_lind_fs_open_empty_directory();
        ut_lind_fs_open_nonexisting_parentdirectory_and_file();
        ut_lind_fs_open_existing_parentdirectory_and_nonexisting_file();
        ut_lind_fs_open_existing_file_without_flags();
        ut_lind_fs_open_existing_file_with_flags();
        ut_lind_fs_open_create_new_file_and_check_link_count();
        ut_lind_fs_open_existing_file_with_o_trunc_flag();
        ut_lind_fs_open_new_file_with_s_ifchar_flag();
    }

    pub fn ut_lind_fs_simple() {
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);

        assert_eq!(cage.access_syscall("/", F_OK), 0);
        assert_eq!(cage.access_syscall("/", X_OK | R_OK), 0);

        let mut statdata2 = StatData::default();

        assert_eq!(cage.stat_syscall("/", &mut statdata2), 0);
        //ensure that there are two hard links

        assert_eq!(statdata2.st_nlink, 5); //2 for . and .., one for dev, and one so that it can never be removed

        //ensure that there is no associated size
        assert_eq!(statdata2.st_size, 0);
        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn rdwrtest() {
        lindrustinit(0);
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

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);

        lindrustfinalize();
    }

    pub fn prdwrtest() {
        lindrustinit(0);
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

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn chardevtest() {
        lindrustinit(0);
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
        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_fs_broken_close() {
        //testing a muck up with the inode table where the regular close does not work as intended

        lindrustinit(0);
        let cage = interface::cagetable_getref(1);

        //write should work
        let mut fd = cage.open_syscall("/broken_close_file", O_CREAT | O_EXCL | O_RDWR, S_IRWXA);
        assert_eq!(cage.write_syscall(fd, str2cbuf("Hello There!"), 12), 12);
        assert_eq!(cage.close_syscall(fd), 0);

        //close the file and then open it again... and then close it again
        fd = cage.open_syscall("/broken_close_file", O_RDWR, S_IRWXA);
        assert_eq!(cage.close_syscall(fd), 0);

        //let's try some things with connect
        //we are going to open a socket with a UDP specification...
        let sockfd = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);

        //bind should not be interesting
        let mut sockad = interface::GenSockaddr::V4(interface::SockaddrV4::default());
        sockad.set_family(AF_INET as u16);
        assert_eq!(cage.bind_syscall(sockfd, &sockad), 0);

        fd = cage.open_syscall("/broken_close_file", O_RDWR, S_IRWXA);
        assert_eq!(cage.close_syscall(fd), 0);

        fd = cage.open_syscall("/broken_close_file", O_RDWR, S_IRWXA);
        assert_eq!(cage.close_syscall(fd), 0);

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_fs_chmod() {
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);

        let flags: i32 = O_TRUNC | O_CREAT | O_RDWR;
        let filepath = "/chmodTestFile";

        let mut statdata = StatData::default();

        let fd = cage.open_syscall(filepath, flags, S_IRWXA);
        assert_eq!(cage.stat_syscall(filepath, &mut statdata), 0);
        assert_eq!(statdata.st_mode, S_IRWXA | S_IFREG as u32);

        assert_eq!(cage.chmod_syscall(filepath, S_IRUSR | S_IRGRP), 0);
        assert_eq!(cage.stat_syscall(filepath, &mut statdata), 0);
        assert_eq!(statdata.st_mode, S_IRUSR | S_IRGRP | S_IFREG as u32);

        assert_eq!(cage.chmod_syscall(filepath, S_IRWXA), 0);
        assert_eq!(cage.stat_syscall(filepath, &mut statdata), 0);
        assert_eq!(statdata.st_mode, S_IRWXA | S_IFREG as u32);

        assert_eq!(cage.close_syscall(fd), 0);
        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_fs_fchmod() {
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);

        let flags: i32 = O_TRUNC | O_CREAT | O_RDWR;
        let filepath = "/fchmodTestFile";

        let mut statdata = StatData::default();

        let fd = cage.open_syscall(filepath, flags, S_IRWXA);
        assert_eq!(cage.fstat_syscall(fd, &mut statdata), 0);
        assert_eq!(statdata.st_mode, S_IRWXA | S_IFREG as u32);

        assert_eq!(cage.fchmod_syscall(fd, S_IRUSR | S_IRGRP), 0);
        assert_eq!(cage.fstat_syscall(fd, &mut statdata), 0);
        assert_eq!(statdata.st_mode, S_IRUSR | S_IRGRP | S_IFREG as u32);

        assert_eq!(cage.fchmod_syscall(fd, S_IRWXA), 0);
        assert_eq!(cage.fstat_syscall(fd, &mut statdata), 0);
        assert_eq!(statdata.st_mode, S_IRWXA | S_IFREG as u32);

        assert_eq!(cage.close_syscall(fd), 0);
        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_fs_dir_chdir() {
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);

        //testing the ability to make and change to directories

        assert_eq!(cage.mkdir_syscall("/subdir1", S_IRWXA), 0);
        assert_eq!(cage.mkdir_syscall("/subdir1/subdir2", S_IRWXA), 0);
        assert_eq!(cage.mkdir_syscall("/subdir1/subdir2/subdir3", 0), 0);

        assert_eq!(cage.access_syscall("subdir1", F_OK), 0);
        assert_eq!(cage.chdir_syscall("subdir1"), 0);

        assert_eq!(cage.access_syscall("subdir2", F_OK), 0);
        assert_eq!(cage.chdir_syscall(".."), 0);

        assert_eq!(cage.access_syscall("subdir1", F_OK), 0);
        assert_eq!(cage.chdir_syscall("/subdir1/subdir2/subdir3"), 0);
        assert_eq!(cage.access_syscall("../../../subdir1", F_OK), 0);

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_fs_dir_mode() {
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);

        let filepath1 = "/subdirDirMode1";
        let filepath2 = "/subdirDirMode2";

        let mut statdata = StatData::default();

        assert_eq!(cage.mkdir_syscall(filepath1, S_IRWXA), 0);
        assert_eq!(cage.stat_syscall(filepath1, &mut statdata), 0);
        assert_eq!(statdata.st_mode, S_IRWXA | S_IFDIR as u32);

        assert_eq!(cage.mkdir_syscall(filepath2, 0), 0);
        assert_eq!(cage.stat_syscall(filepath2, &mut statdata), 0);
        assert_eq!(statdata.st_mode, S_IFDIR as u32);

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_fs_dir_multiple() {
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);

        assert_eq!(cage.mkdir_syscall("/subdirMultiple1", S_IRWXA), 0);
        assert_eq!(
            cage.mkdir_syscall("/subdirMultiple1/subdirMultiple2", S_IRWXA),
            0
        );
        assert_eq!(
            cage.mkdir_syscall("/subdirMultiple1/subdirMultiple2/subdirMultiple3", 0),
            0
        );

        let mut statdata = StatData::default();

        //ensure that the file is a dir with all of the correct bits on for nodes
        assert_eq!(
            cage.stat_syscall("/subdirMultiple1/subdirMultiple2", &mut statdata),
            0
        );
        assert_eq!(statdata.st_mode, S_IRWXA | S_IFDIR as u32);

        assert_eq!(
            cage.stat_syscall(
                "/subdirMultiple1/subdirMultiple2/subdirMultiple3",
                &mut statdata
            ),
            0
        );
        assert_eq!(statdata.st_mode, S_IFDIR as u32);

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_fs_dup() {
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);

        let flags: i32 = O_TRUNC | O_CREAT | O_RDWR;
        let filepath = "/dupfile";

        let fd = cage.open_syscall(filepath, flags, S_IRWXA);
        let mut temp_buffer = sizecbuf(2);
        assert!(fd >= 0);
        assert_eq!(cage.write_syscall(fd, str2cbuf("12"), 2), 2);
        assert_eq!(cage.lseek_syscall(fd, 0, SEEK_SET), 0);
        assert_eq!(cage.read_syscall(fd, temp_buffer.as_mut_ptr(), 2), 2);
        assert_eq!(cbuf2str(&temp_buffer), "12");

        //duplicate the file descriptor
        let fd2 = cage.dup_syscall(fd, None);
        assert!(fd != fd2);

        //essentially a no-op, but duplicate again -- they should be diff &fd's
        let fd3 = cage.dup_syscall(fd, None);
        assert!(fd != fd2 && fd != fd3);

        //We don't need all three, though:
        assert_eq!(cage.close_syscall(fd3), 0);

        assert_eq!(cage.lseek_syscall(fd, 0, SEEK_END), 2);
        assert_eq!(cage.lseek_syscall(fd2, 0, SEEK_END), 2);

        // write some data to move the first position
        assert_eq!(cage.write_syscall(fd, str2cbuf("34"), 2), 2);

        //Make sure that they are still in the same place:
        let mut buffer = sizecbuf(4);
        assert_eq!(
            cage.lseek_syscall(fd, 0, SEEK_SET),
            cage.lseek_syscall(fd2, 0, SEEK_SET)
        );
        assert_eq!(cage.read_syscall(fd, buffer.as_mut_ptr(), 4), 4);
        assert_eq!(cbuf2str(&buffer), "1234");

        assert_eq!(cage.close_syscall(fd), 0);

        //the other &fd should still work
        assert_eq!(cage.lseek_syscall(fd2, 0, SEEK_END), 4);
        assert_eq!(cage.write_syscall(fd2, str2cbuf("5678"), 4), 4);

        assert_eq!(cage.lseek_syscall(fd2, 0, SEEK_SET), 0);
        let mut buffer2 = sizecbuf(8);
        assert_eq!(cage.read_syscall(fd2, buffer2.as_mut_ptr(), 8), 8);
        assert_eq!(cage.close_syscall(fd2), 0);
        assert_eq!(cbuf2str(&buffer2), "12345678");

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_fs_dup2() {
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);

        let flags: i32 = O_TRUNC | O_CREAT | O_RDWR;
        let filepath = "/dup2file";

        let fd = cage.open_syscall(filepath, flags, S_IRWXA);

        assert_eq!(cage.write_syscall(fd, str2cbuf("12"), 2), 2);

        //trying to dup fd into fd + 1
        let _fd2: i32 = cage.dup2_syscall(fd, fd + 1 as i32);

        //should be a no-op since the last line did the same thing
        let fd2: i32 = cage.dup2_syscall(fd, fd + 1 as i32);

        //read/write tests for the files
        assert_eq!(
            cage.lseek_syscall(fd, 0, SEEK_END),
            cage.lseek_syscall(fd2, 0, SEEK_END)
        );
        assert_eq!(cage.write_syscall(fd, str2cbuf("34"), 2), 2);
        assert_eq!(
            cage.lseek_syscall(fd, 0, SEEK_SET),
            cage.lseek_syscall(fd2, 0, SEEK_SET)
        );

        let mut buffer = sizecbuf(4);
        assert_eq!(cage.lseek_syscall(fd2, 0, SEEK_SET), 0);
        assert_eq!(cage.read_syscall(fd, buffer.as_mut_ptr(), 4), 4);
        assert_eq!(cbuf2str(&buffer), "1234");

        assert_eq!(cage.close_syscall(fd), 0);

        let mut buffer2 = sizecbuf(8);
        assert_eq!(cage.lseek_syscall(fd2, 0, SEEK_END), 4);
        assert_eq!(cage.write_syscall(fd2, str2cbuf("5678"), 4), 4);

        assert_eq!(cage.lseek_syscall(fd2, 0, SEEK_SET), 0);
        assert_eq!(cage.read_syscall(fd2, buffer2.as_mut_ptr(), 8), 8);
        assert_eq!(cbuf2str(&buffer2), "12345678");

        assert_eq!(cage.close_syscall(fd2), 0);
        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_fs_fcntl_valid_args() {
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);

        let sockfd = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        let filefd = cage.open_syscall("/fcntl_file_1", O_CREAT | O_EXCL, S_IRWXA);

        //changing O_CLOEXEC file descriptor flag and checking if it was correctly set
        assert_eq!(cage.fcntl_syscall(sockfd, F_SETFD, O_CLOEXEC), 0);
        assert_eq!(cage.fcntl_syscall(sockfd, F_GETFD, 0), O_CLOEXEC);

        //changing the file access mode to read-only, enabling the 
        //O_NONBLOCK file status flag, and checking if they were correctly set
        assert_eq!(cage.fcntl_syscall(filefd, F_SETFL, O_RDONLY | O_NONBLOCK), 0);
        assert_eq!(cage.fcntl_syscall(filefd, F_GETFL, 0), 2048);

        //when provided with 'F_GETFD' or 'F_GETFL' command, 'arg' should be ignored, thus even
        //negative arg values should produce nomal behavior
        assert_eq!(cage.fcntl_syscall(sockfd, F_GETFD, -132), O_CLOEXEC);
        assert_eq!(cage.fcntl_syscall(filefd, F_GETFL, -1998), 2048);

        assert_eq!(cage.close_syscall(filefd), 0);
        assert_eq!(cage.close_syscall(sockfd), 0);

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_fs_fcntl_invalid_args(){
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);
        let filefd = cage.open_syscall("/fcntl_file_2", O_CREAT | O_EXCL, S_IRWXA);
        //when presented with a nonexistent command, 'Invalid Argument' error should be thrown
        //29 is an arbitrary number that does not correspond to any of the defined 'fcntl' commands
        assert_eq!(cage.fcntl_syscall(filefd, 29, 0), -(Errno::EINVAL as i32));
        //when a negative arg is provided with F_SETFD, F_SETFL, or F_DUPFD,
        //Invalid Argument' error should be thrown as well
        assert_eq!(cage.fcntl_syscall(filefd, F_SETFD, -5), -(Errno::EINVAL as i32));
        assert_eq!(cage.fcntl_syscall(filefd, F_SETFL, -5), -(Errno::EINVAL as i32));
        assert_eq!(cage.fcntl_syscall(filefd, F_DUPFD, -5), -(Errno::EINVAL as i32));

        assert_eq!(cage.close_syscall(filefd), 0);

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_fs_fcntl_dup(){
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);

        let filefd1 = cage.open_syscall("/fcntl_file_4", O_CREAT | O_EXCL | O_RDWR, S_IRWXA);
        //on success, returning the new file descriptor greater than or equal to 100
        //and different from the original file descriptor
        let filefd2 = cage.fcntl_syscall(filefd1, F_DUPFD, 100);
        assert!(filefd2 >= 100 && filefd2 != filefd1);

        //to check if both file descriptors refer to the same fie, we can write into a file
        //using one file descriptor, read from the file using another file descriptor,
        //and make sure that the contents are the same
        let mut temp_buffer = sizecbuf(9);
        assert_eq!(cage.write_syscall(filefd1, str2cbuf("Test text"), 9), 9);
        assert_eq!(cage.read_syscall(filefd2, temp_buffer.as_mut_ptr(), 9), 9);
        assert_eq!(cbuf2str(&temp_buffer), "Test text");

        //file status flags are shared by duplicated file descriptors resulting from
        //a single opening of the file
        assert_eq!(cage.fcntl_syscall(filefd1, F_GETFL, 0), cage.fcntl_syscall(filefd2, F_GETFL, 0));

        assert_eq!(cage.close_syscall(filefd1), 0);
        assert_eq!(cage.close_syscall(filefd2), 0);

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }


    pub fn ut_lind_fs_ioctl_valid_args() {
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);

        //setting up two integer values (a zero value to test clearing nonblocking I/O behavior
        //and a non-zero value to test setting nonblocking I/O behavior)
        let mut arg0: i32 = 0;
        let mut arg1: i32 = 1;

        //ioctl requires a pointer to an integer to be passed with FIONBIO command
        let union0: IoctlPtrUnion = IoctlPtrUnion { int_ptr: &mut arg0 };
        let union1: IoctlPtrUnion = IoctlPtrUnion { int_ptr: &mut arg1 };

        let sockfd = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);

        //calling ioctl with FIONBIO command and a pointer to a zero-valued integer
        //to clear the socket's nonblocking I/O, and checking if the flag was correctly set 
        assert_eq!(cage.ioctl_syscall(sockfd, FIONBIO, union0), 0);
        assert_eq!(cage.fcntl_syscall(sockfd, F_GETFL, 0) & O_NONBLOCK, 0);

        //calling ioctl with FIONBIO command and a pointer to a non-zero-valued integer
        //to set the socket's nonblocking I/O, and checking if the flag was correctly set
        assert_eq!(cage.ioctl_syscall(sockfd, FIONBIO, union1), 0);
        assert_eq!(cage.fcntl_syscall(sockfd, F_GETFL, 0) & O_NONBLOCK, O_NONBLOCK);

        assert_eq!(cage.close_syscall(sockfd), 0);

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_fs_ioctl_invalid_args() {
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);

        //setting up two integer values (a zero value to test clearing nonblocking I/O behavior on
        //non-socket type and a non-zero value to test setting nonblocking I/O behavior 
        //on non-socket type)
        let mut arg0: i32 = 0;
        let mut arg1: i32 = 1;

        //ioctl requires a pointer to an integer to be passed with FIONBIO command
        let union0: IoctlPtrUnion = IoctlPtrUnion { int_ptr: &mut arg0 };
        let union1: IoctlPtrUnion = IoctlPtrUnion { int_ptr: &mut arg1 };

        let sockfd = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        let filefd = cage.open_syscall("/ioctl_file", O_CREAT | O_EXCL, S_IRWXA);

        //trying to use FIONBIO command on a non-socket type (the file type in this case) 
        //for any 'ptrunion' value should throw a 'Not a typewriter' error
        assert_eq!(cage.ioctl_syscall(filefd, FIONBIO, union0), -(Errno::ENOTTY as i32));
        assert_eq!(cage.ioctl_syscall(filefd, FIONBIO, union1), -(Errno::ENOTTY as i32));
        assert_eq!(cage.close_syscall(filefd), 0);

        //calling 'ioctl' with a control function that is not implemented yet should
        //return an 'Invalid argument' error
        //21600 is an arbitrary integer that does not correspond to any implemented
        //control functions for ioctl syscall
        assert_eq!(cage.ioctl_syscall(sockfd, 21600, union0), -(Errno::EINVAL as i32));

        //calling ioctl with FIONBIO command and a null pointer 
        //should return a 'Bad address' error
        let null_ptr: *mut i32 = std::ptr::null_mut();
        let union_null: IoctlPtrUnion = IoctlPtrUnion { int_ptr: null_ptr };
        assert_eq!(cage.ioctl_syscall(sockfd, FIONBIO, union_null),-(Errno::EFAULT as i32));

        //calling ioctl on a closed file descriptor should throw a 'Bad file number' error
        assert_eq!(cage.close_syscall(sockfd), 0);
        assert_eq!(cage.fcntl_syscall(sockfd, F_GETFL, 0), -(Errno::EBADF as i32));

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_fs_fdflags() {
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);

        let path = "/fdFlagsFile";

        let fd = cage.creat_syscall(path, S_IRWXA);
        assert_eq!(cage.close_syscall(fd), 0);

        let read_fd = cage.open_syscall(path, O_RDONLY, S_IRWXA);
        assert_eq!(cage.lseek_syscall(read_fd, 0, SEEK_SET), 0);
        assert_eq!(
            cage.write_syscall(read_fd, str2cbuf("Hello! This should not write."), 28),
            -(Errno::EBADF as i32)
        );

        let mut buf = sizecbuf(100);
        assert_eq!(cage.lseek_syscall(read_fd, 0, SEEK_SET), 0);

        //this fails because nothing is written to the readfd (the previous write was unwritable)
        assert_eq!(cage.read_syscall(read_fd, buf.as_mut_ptr(), 100), 0);
        assert_eq!(cage.close_syscall(read_fd), 0);

        let write_fd = cage.open_syscall(path, O_WRONLY, S_IRWXA);
        let mut buf2 = sizecbuf(100);
        assert_eq!(cage.lseek_syscall(write_fd, 0, SEEK_SET), 0);
        assert_eq!(
            cage.read_syscall(write_fd, buf2.as_mut_ptr(), 100),
            -(Errno::EBADF as i32)
        );

        assert_eq!(cage.lseek_syscall(write_fd, 0, SEEK_SET), 0);
        assert_eq!(
            cage.write_syscall(write_fd, str2cbuf("Hello! This should write."), 24),
            24
        );
        assert_eq!(cage.close_syscall(write_fd), 0);

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_fs_file_link_unlink() {
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);

        let path = "/fileLink";
        let path2 = "/fileLink2";

        let fd = cage.open_syscall(path, O_CREAT | O_EXCL | O_WRONLY, S_IRWXA);
        assert_eq!(cage.lseek_syscall(fd, 0, SEEK_SET), 0);
        assert_eq!(cage.write_syscall(fd, str2cbuf("hi"), 2), 2);

        let mut statdata = StatData::default();

        assert_eq!(cage.stat_syscall(path, &mut statdata), 0);
        assert_eq!(statdata.st_size, 2);
        assert_eq!(statdata.st_nlink, 1);

        let mut statdata2 = StatData::default();

        //make sure that this has the same traits as the other file that we linked
        // and make sure that the link count on the orig file has increased
        assert_eq!(cage.link_syscall(path, path2), 0);
        assert_eq!(cage.stat_syscall(path, &mut statdata), 0);
        assert_eq!(cage.stat_syscall(path2, &mut statdata2), 0);
        assert!(statdata == statdata2);
        assert_eq!(statdata.st_nlink, 2);

        //now we unlink
        assert_eq!(cage.unlink_syscall(path), 0);
        assert_eq!(cage.stat_syscall(path2, &mut statdata2), 0);
        assert_eq!(statdata2.st_nlink, 1);

        //it shouldn't work to stat the orig since it is gone
        assert_ne!(cage.stat_syscall(path, &mut statdata), 0);
        assert_eq!(cage.unlink_syscall(path2), 0);

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_fs_file_lseek_past_end() {
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);

        let path = "/lseekPastEnd";

        let fd = cage.open_syscall(path, O_CREAT | O_EXCL | O_RDWR, S_IRWXA);
        assert_eq!(cage.write_syscall(fd, str2cbuf("hello"), 5), 5);

        //seek past the end and then write
        assert_eq!(cage.lseek_syscall(fd, 10, SEEK_SET), 10);
        assert_eq!(cage.write_syscall(fd, str2cbuf("123456"), 6), 6);

        let mut buf = sizecbuf(16);
        assert_eq!(cage.lseek_syscall(fd, 0, SEEK_SET), 0);
        assert_eq!(cage.read_syscall(fd, buf.as_mut_ptr(), 20), 16);
        assert_eq!(cbuf2str(&buf), "hello\0\0\0\0\0123456");

        assert_eq!(cage.close_syscall(fd), 0);
        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_fs_fstat_complex() {
        lindrustinit(0);

        let cage = interface::cagetable_getref(1);
        let path = "/complexFile";

        let fd = cage.open_syscall(path, O_CREAT | O_WRONLY, S_IRWXA);
        assert_eq!(cage.write_syscall(fd, str2cbuf("testing"), 4), 4);

        let mut statdata = StatData::default();

        assert_eq!(cage.fstat_syscall(fd, &mut statdata), 0);
        assert_eq!(statdata.st_size, 4);
        assert_eq!(statdata.st_nlink, 1);

        assert_eq!(cage.close_syscall(fd), 0);
        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_fs_getuid() {
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);

        //let's get the initial -1s out of the way
        cage.getgid_syscall();
        cage.getegid_syscall();
        cage.getuid_syscall();
        cage.geteuid_syscall();

        //testing to make sure that all of the gid and uid values are good to go when system is initialized
        assert_eq!(cage.getgid_syscall() as u32, DEFAULT_GID);
        assert_eq!(cage.getegid_syscall() as u32, DEFAULT_GID);
        assert_eq!(cage.getuid_syscall() as u32, DEFAULT_UID);
        assert_eq!(cage.geteuid_syscall() as u32, DEFAULT_UID);

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_fs_load_fs() {
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);

        let mut statdata = StatData::default();

        //testing that all of the dev files made it out safe and sound
        cage.stat_syscall("/dev", &mut statdata);

        assert_eq!(cage.stat_syscall("/dev/null", &mut statdata), 0);
        assert_eq!(statdata.st_rdev, makedev(&DevNo { major: 1, minor: 3 }));

        assert_eq!(cage.stat_syscall("/dev/random", &mut statdata), 0);
        assert_eq!(statdata.st_rdev, makedev(&DevNo { major: 1, minor: 8 }));

        assert_eq!(cage.stat_syscall("/dev/urandom", &mut statdata), 0);
        assert_eq!(statdata.st_rdev, makedev(&DevNo { major: 1, minor: 9 }));

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_fs_mknod() {
        // let's create /dev/null
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);
        let dev = makedev(&DevNo { major: 1, minor: 3 });
        let path = "/null";

        //now we are going to mknod /dev/null with create, read and write flags and permissions
        //and then makr sure that it exists
        assert_eq!(cage.mknod_syscall(path, S_IFCHR as u32, dev), 0);
        let fd = cage.open_syscall(path, O_RDWR, S_IRWXA);

        //checking the metadata of the file:
        let mut statdata = StatData::default();

        //should be a chr file, so let's check this
        let mut buf = sizecbuf(4);
        assert_eq!(cage.fstat_syscall(fd, &mut statdata), 0);
        assert_eq!(statdata.st_mode & S_FILETYPEFLAGS as u32, S_IFCHR as u32);
        assert_eq!(statdata.st_rdev, dev);
        assert_eq!(cage.write_syscall(fd, str2cbuf("test"), 4), 4);
        assert_eq!(cage.read_syscall(fd, buf.as_mut_ptr(), 4), 0);
        assert_eq!(cbuf2str(&buf), "\0\0\0\0");
        assert_eq!(cage.close_syscall(fd), 0);

        let mut statdata2 = StatData::default();

        //try it again with /dev/random
        let dev2 = makedev(&DevNo { major: 1, minor: 8 });
        let path2 = "/random";

        //making the node and then making sure that it exists
        assert_eq!(cage.mknod_syscall(path2, S_IFCHR as u32, dev2), 0);
        let fd2 = cage.open_syscall(path2, O_RDWR, S_IRWXA);

        let mut buf2 = sizecbuf(4);
        assert_eq!(cage.fstat_syscall(fd2, &mut statdata2), 0);
        assert_eq!(statdata2.st_mode & S_FILETYPEFLAGS as u32, S_IFCHR as u32);
        assert_eq!(statdata2.st_rdev, dev2);
        assert_eq!(cage.write_syscall(fd2, str2cbuf("testing"), 7), 7);
        assert_ne!(cage.read_syscall(fd2, buf2.as_mut_ptr(), 7), 0);
        assert_eq!(cage.close_syscall(fd2), 0);

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_fs_multiple_open() {
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);

        //try to open several files at once -- the fd's should not be overwritten
        let fd1 = cage.open_syscall("/foo", O_CREAT | O_EXCL | O_RDWR, S_IRWXA);
        let fd2 = cage.open_syscall("/foo", O_RDWR, S_IRWXA);
        assert_ne!(fd1, fd2);

        let flags: i32 = O_TRUNC | O_CREAT | O_RDWR;
        let mode: u32 = 0o666; // 0666
        let name = "double_open_file";

        let mut read_buf = sizecbuf(2);
        let fd3 = cage.open_syscall(name, flags, mode);
        assert_eq!(cage.write_syscall(fd3, str2cbuf("hi"), 2), 2);
        assert_eq!(cage.lseek_syscall(fd3, 0, SEEK_SET), 0);
        assert_eq!(cage.read_syscall(fd3, read_buf.as_mut_ptr(), 2), 2);
        assert_eq!(cbuf2str(&read_buf), "hi");

        let _fd4 = cage.open_syscall(name, flags, mode);
        let mut buf = sizecbuf(5);
        assert_eq!(cage.lseek_syscall(fd3, 2, SEEK_SET), 2);
        assert_eq!(cage.write_syscall(fd3, str2cbuf("boo"), 3), 3);
        assert_eq!(cage.lseek_syscall(fd3, 0, SEEK_SET), 0);
        assert_eq!(cage.read_syscall(fd3, buf.as_mut_ptr(), 5), 5);
        assert_eq!(cbuf2str(&buf), "\0\0boo");

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_fs_rmdir() {
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);

        let path = "/parent_dir/dir";
        assert_eq!(cage.mkdir_syscall("/parent_dir", S_IRWXA), 0);
        assert_eq!(cage.mkdir_syscall(path, S_IRWXA), 0);
        assert_eq!(cage.rmdir_syscall(path), 0);

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_fs_stat_file_complex() {
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);
        let fd = cage.open_syscall("/fooComplex", O_CREAT | O_EXCL | O_WRONLY, S_IRWXA);

        assert_eq!(cage.write_syscall(fd, str2cbuf("hi"), 2), 2);

        let mut statdata = StatData::default();
        let mut statdata2 = StatData::default();

        assert_eq!(cage.fstat_syscall(fd, &mut statdata), 0);
        assert_eq!(statdata.st_size, 2);
        assert_eq!(statdata.st_nlink, 1);

        assert_eq!(cage.link_syscall("/fooComplex", "/barComplex"), 0);
        assert_eq!(cage.stat_syscall("/fooComplex", &mut statdata), 0);
        assert_eq!(cage.stat_syscall("/barComplex", &mut statdata2), 0);

        //check that they are the same and that the link count is 0
        assert!(statdata == statdata2);
        assert_eq!(statdata.st_nlink, 2);

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_fs_stat_file_mode() {
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);
        let path = "/fooFileMode";
        let _fd = cage.open_syscall(path, O_CREAT | O_EXCL | O_WRONLY, S_IRWXA);

        let mut statdata = StatData::default();
        assert_eq!(cage.stat_syscall(path, &mut statdata), 0);
        assert_eq!(statdata.st_mode, S_IRWXA | S_IFREG as u32);

        //make a file without permissions and check that it is a reg file without permissions
        let path2 = "/fooFileMode2";
        let _fd2 = cage.open_syscall(path2, O_CREAT | O_EXCL | O_WRONLY, 0);
        assert_eq!(cage.stat_syscall(path2, &mut statdata), 0);
        assert_eq!(statdata.st_mode, S_IFREG as u32);

        //check that stat can be done on the current (root) dir
        assert_eq!(cage.stat_syscall(".", &mut statdata), 0);

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_fs_statfs() {
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);
        let mut fsdata = FSData::default();

        assert_eq!(cage.statfs_syscall("/", &mut fsdata), 0);
        assert_eq!(fsdata.f_type, 0xBEEFC0DE);
        assert_eq!(fsdata.f_bsize, 4096);

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_fs_fstatfs() {
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);
        let mut fsdata = FSData::default();

        // Get fd
        let fd = cage.open_syscall("/", O_RDONLY, 0);
        assert!(fd >= 0);
        // fstatfs
        assert_eq!(cage.fstatfs_syscall(fd, &mut fsdata), 0);
        // Check the output
        assert_eq!(fsdata.f_type, 0xBEEFC0DE);
        assert_eq!(fsdata.f_bsize, 4096);
        // Close the file
        assert_eq!(cage.close_syscall(fd), 0);

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_fs_rename() {
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);

        let old_path = "/test_dir";
        assert_eq!(cage.mkdir_syscall(old_path, S_IRWXA), 0);
        assert_eq!(cage.rename_syscall(old_path, "/test_dir_renamed"), 0);

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_fs_ftruncate() {
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);

        let fd = cage.open_syscall("/ftruncate", O_CREAT | O_TRUNC | O_RDWR, S_IRWXA);
        assert!(fd >= 0);

        // check if ftruncate() works for extending file with null bytes
        assert_eq!(cage.write_syscall(fd, str2cbuf("Hello there!"), 12), 12);
        assert_eq!(cage.ftruncate_syscall(fd, 15), 0);
        assert_eq!(cage.lseek_syscall(fd, 0, SEEK_SET), 0);
        let mut buf = sizecbuf(15);
        assert_eq!(cage.read_syscall(fd, buf.as_mut_ptr(), 15), 15);
        assert_eq!(cbuf2str(&buf), "Hello there!\0\0\0");

        // check if ftruncate() works for cutting off extra bytes
        assert_eq!(cage.ftruncate_syscall(fd, 5), 0);
        assert_eq!(cage.lseek_syscall(fd, 0, SEEK_SET), 0);
        let mut buf1 = sizecbuf(7);
        assert_eq!(cage.read_syscall(fd, buf1.as_mut_ptr(), 7), 5);
        assert_eq!(cbuf2str(&buf1), "Hello\0\0");

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_fs_truncate() {
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);

        let path = String::from("/truncate");
        let fd = cage.open_syscall(&path, O_CREAT | O_TRUNC | O_RDWR, S_IRWXA);
        assert!(fd >= 0);

        // check if truncate() works for extending file with null bytes
        assert_eq!(cage.write_syscall(fd, str2cbuf("Hello there!"), 12), 12);
        assert_eq!(cage.truncate_syscall(&path, 15), 0);
        assert_eq!(cage.lseek_syscall(fd, 0, SEEK_SET), 0);
        let mut buf = sizecbuf(15);
        assert_eq!(cage.read_syscall(fd, buf.as_mut_ptr(), 15), 15);
        assert_eq!(cbuf2str(&buf), "Hello there!\0\0\0");

        // check if truncate() works for cutting off extra bytes
        assert_eq!(cage.truncate_syscall(&path, 5), 0);
        assert_eq!(cage.lseek_syscall(fd, 0, SEEK_SET), 0);
        let mut buf1 = sizecbuf(7);
        assert_eq!(cage.read_syscall(fd, buf1.as_mut_ptr(), 7), 5);
        assert_eq!(cbuf2str(&buf1), "Hello\0\0");

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[cfg(target_os = "macos")]
    type CharPtr = *const u8;

    #[cfg(not(target_os = "macos"))]
    type CharPtr = *const i8;

    pub fn ut_lind_fs_getdents() {
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);

        let bufsize = 50;
        let mut vec = vec![0u8; bufsize as usize];
        let baseptr: *mut u8 = &mut vec[0];

        assert_eq!(cage.mkdir_syscall("/getdents", S_IRWXA), 0);
        let fd = cage.open_syscall("/getdents", O_RDWR, S_IRWXA);
        assert_eq!(cage.getdents_syscall(fd, baseptr, bufsize as u32), 48);

        unsafe {
            let first_dirent = baseptr as *mut interface::ClippedDirent;
            assert!((*first_dirent).d_off == 24);
            let reclen_matched: bool = ((*first_dirent).d_reclen == 24);
            assert_eq!(reclen_matched, true);

            let nameoffset = baseptr.wrapping_offset(interface::CLIPPED_DIRENT_SIZE as isize);
            let returnedname = interface::RustCStr::from_ptr(nameoffset as *const _);
            let name_matched: bool = (returnedname
                == interface::RustCStr::from_bytes_with_nul(b".\0").unwrap())
                | (returnedname == interface::RustCStr::from_bytes_with_nul(b"..\0").unwrap());
            assert_eq!(name_matched, true);

            let second_dirent = baseptr.wrapping_offset(24) as *mut interface::ClippedDirent;
            assert!((*second_dirent).d_off >= 48);
        }

        assert_eq!(cage.close_syscall(fd), 0);
        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_fs_dir_chdir_getcwd() {
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);
        let needed = "/subdir1\0".as_bytes().to_vec().len();

        let needed_u32: u32 = needed as u32;

        let mut buf = vec![0u8; needed];
        let bufptr: *mut u8 = &mut buf[0];

        assert_eq!(cage.chdir_syscall("/"), 0);
        assert_eq!(cage.getcwd_syscall(bufptr, 0), -(Errno::ERANGE as i32));
        assert_eq!(cage.getcwd_syscall(bufptr, 1), -(Errno::ERANGE as i32));
        assert_eq!(cage.getcwd_syscall(bufptr, 2), 0);
        assert_eq!(std::str::from_utf8(&buf).unwrap(), "/\0\0\0\0\0\0\0\0");

        cage.mkdir_syscall("/subdir1", S_IRWXA);
        assert_eq!(cage.access_syscall("subdir1", F_OK), 0);
        assert_eq!(cage.chdir_syscall("subdir1"), 0);

        assert_eq!(cage.getcwd_syscall(bufptr, 0), -(Errno::ERANGE as i32));
        assert_eq!(
            cage.getcwd_syscall(bufptr, needed_u32 - 1),
            -(Errno::ERANGE as i32)
        );
        assert_eq!(cage.getcwd_syscall(bufptr, needed_u32), 0);
        assert_eq!(std::str::from_utf8(&buf).unwrap(), "/subdir1\0");

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_fs_exec_cloexec() {
        lindrustinit(0);
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

        assert_eq!(cage.exec_syscall(2), 0);

        let execcage = interface::cagetable_getref(2);
        assert_eq!(
            execcage.fstat_syscall(fd1, &mut uselessstatdata),
            -(Errno::EBADF as i32)
        );
        assert_eq!(execcage.fstat_syscall(fd2, &mut uselessstatdata), 0);

        assert_eq!(execcage.close_syscall(fd2), 0);
        assert_eq!(cage.unlink_syscall("/cloexecuted"), 0);
        assert_eq!(cage.unlink_syscall("/cloexekept"), 0);

        assert_eq!(execcage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_fs_shm() {
        lindrustinit(0);
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

    pub fn ut_lind_fs_getpid_getppid() {
        lindrustinit(0);

        let cage1 = interface::cagetable_getref(1);
        let pid1 = cage1.getpid_syscall();

        assert_eq!(cage1.fork_syscall(2), 0);

        let child = std::thread::spawn(move || {
            let cage2 = interface::cagetable_getref(2);
            let pid2 = cage2.getpid_syscall();
            let ppid2 = cage2.getppid_syscall();

            assert_ne!(pid2, pid1); // make sure the child and the parent have different pids
            assert_eq!(ppid2, pid1); // make sure the child's getppid is correct

            assert_eq!(cage2.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        });

        child.join().unwrap();
        assert_eq!(cage1.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_fs_sem_fork() {
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);
        let key = 31337;
        // Create a shared memory region
        let shmid = cage.shmget_syscall(key, 1024, 0666 | IPC_CREAT);
        // Attach the shared memory region
        let shmatret = cage.shmat_syscall(shmid, 0xfffff000 as *mut u8, 0);
        assert_ne!(shmatret, -1);
        // Initialize the semaphore with shared between process
        let ret_init = cage.sem_init_syscall(shmatret as u32, 1, 1);
        assert_eq!(ret_init, 0);
        assert_eq!(cage.sem_getvalue_syscall(shmatret as u32), 1);
        // Fork child process
        assert_eq!(cage.fork_syscall(2), 0);
        // Child process
        let thread_child = interface::helper_thread(move || {
            let cage1 = interface::cagetable_getref(2);
            // Child waits for the semaphore
            assert_eq!(cage1.sem_wait_syscall(shmatret as u32), 0);
            interface::sleep(interface::RustDuration::from_millis(40));
            // Release the semaphore
            assert_eq!(cage1.sem_post_syscall(shmatret as u32), 0);
            cage1.exit_syscall(EXIT_SUCCESS);
        });

        //Parent processes
        // Parents waits for the semaphore
        assert_eq!(cage.sem_wait_syscall(shmatret as u32), 0);
        assert_eq!(cage.sem_getvalue_syscall(shmatret as u32), 0);
        interface::sleep(interface::RustDuration::from_millis(100));

        // Parents release the semaphore
        assert_eq!(cage.sem_post_syscall(shmatret as u32), 0);

        // wait for the child process to exit before destroying the semaphore.
        thread_child.join().unwrap();

        // Destroy the semaphore
        assert_eq!(cage.sem_destroy_syscall(shmatret as u32), 0);
        // mark the shared memory to be rmoved
        let shmctlret2 = cage.shmctl_syscall(shmid, IPC_RMID, None);
        assert_eq!(shmctlret2, 0);
        //detach from shared memory
        let shmdtret = cage.shmdt_syscall(0xfffff000 as *mut u8);
        assert_eq!(shmdtret, shmid);
        cage.exit_syscall(EXIT_SUCCESS);

        lindrustfinalize();
    }

    pub fn ut_lind_fs_sem_trytimed() {
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);
        let key = 31337;
        // Create a shared memory region
        let shmid = cage.shmget_syscall(key, 1024, 0666 | IPC_CREAT);
        // Attach the shared memory region
        let shmatret = cage.shmat_syscall(shmid, 0xfffff000 as *mut u8, 0);
        assert_ne!(shmatret, -1);
        // Initialize the semaphore with shared between process
        let ret_init = cage.sem_init_syscall(shmatret as u32, 1, 1);
        // assert_eq!(shmatret as u32, 0);
        assert_eq!(ret_init, 0);
        assert_eq!(cage.sem_getvalue_syscall(shmatret as u32), 1);
        // Fork child process
        assert_eq!(cage.fork_syscall(2), 0);
        // Child process
        let thread_child = interface::helper_thread(move || {
            let cage1 = interface::cagetable_getref(2);
            // Child waits for the semaphore
            assert_eq!(cage1.sem_wait_syscall(shmatret as u32), 0);
            // Wait
            interface::sleep(interface::RustDuration::from_millis(20));
            // Release the semaphore
            assert_eq!(cage1.sem_post_syscall(shmatret as u32), 0);
            cage1.exit_syscall(EXIT_SUCCESS);
        });
        //Parent processes
        // Parents waits for the semaphore
        assert_eq!(
            cage.sem_timedwait_syscall(
                shmatret as u32,
                interface::RustDuration::from_millis(100)
            ),
            0
        );
        assert_eq!(cage.sem_getvalue_syscall(shmatret as u32), 0);
        interface::sleep(interface::RustDuration::from_millis(10));
        // Parents release the semaphore
        assert_eq!(cage.sem_post_syscall(shmatret as u32), 0);
        
        // wait for the child to exit before destroying the semaphore.
        thread_child.join().unwrap();

        // Destroy the semaphore
        assert_eq!(cage.sem_destroy_syscall(shmatret as u32), 0);
        // mark the shared memory to be rmoved
        let shmctlret2 = cage.shmctl_syscall(shmid, IPC_RMID, None);
        assert_eq!(shmctlret2, 0);
        //detach from shared memory
        let shmdtret = cage.shmdt_syscall(0xfffff000 as *mut u8);
        assert_eq!(shmdtret, shmid);

        cage.exit_syscall(EXIT_SUCCESS);

        lindrustfinalize();
    }

    pub fn ut_lind_fs_sem_test() {
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);
        let key = 31337;
        // Create a shared memory region
        let shmid = cage.shmget_syscall(key, 1024, 0666 | IPC_CREAT);
        // Attach the shared memory region
        let shmatret = cage.shmat_syscall(shmid, 0xfffff000 as *mut u8, 0);
        assert_ne!(shmatret, -1);
        assert_eq!(cage.sem_destroy_syscall(shmatret as u32), -22);
        assert_eq!(cage.sem_getvalue_syscall(shmatret as u32), -22);
        assert_eq!(cage.sem_post_syscall(shmatret as u32), -22);
        // Initialize the semaphore with shared between process
        let ret_init = cage.sem_init_syscall(shmatret as u32, 1, 0);
        assert_eq!(ret_init, 0);
        // Should return errno
        assert_eq!(
            cage.sem_timedwait_syscall(shmatret as u32, interface::RustDuration::from_millis(100)),
            -110
        );
        assert_eq!(cage.sem_trywait_syscall(shmatret as u32), -11);
        lindrustfinalize();
    }

    pub fn ut_lind_fs_tmp_file_test() {
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);

        // Check if /tmp is there
        assert_eq!(cage.access_syscall("/tmp", F_OK), 0);

        // Open  file in /tmp
        let file_path = "/tmp/testfile";
        let fd = cage.open_syscall(file_path, O_CREAT | O_TRUNC | O_RDWR, S_IRWXA);

        assert_eq!(cage.write_syscall(fd, str2cbuf("Hello world"), 6), 6);
        assert_eq!(cage.close_syscall(fd), 0);

        lindrustfinalize();

        // Init again
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);

        // Check if /tmp is there
        assert_eq!(cage.access_syscall("/tmp", F_OK), 0);
        // Check if file is still there (it shouldn't be, assert no)
        assert_eq!(cage.access_syscall(file_path, F_OK), -2);

        lindrustfinalize();
    }   

    pub fn ut_lind_fs_mkdir_empty_directory() {
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);
        let path = "";
        // Check for error when directory is empty
        assert_eq!(cage.mkdir_syscall(path, S_IRWXA), -(Errno::ENOENT as i32));
        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_fs_mkdir_nonexisting_directory() {
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);
        let path = "/parentdir/dir";
        // Check for error when both parent and child directories don't exist 
        assert_eq!(cage.mkdir_syscall(path, S_IRWXA), -(Errno::ENOENT as i32));
        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_fs_mkdir_existing_directory() {
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);
        let path = "/parentdir";
        // Create a parent directory
        cage.mkdir_syscall(path, S_IRWXA);
        // Check for error when the same directory is created again
        assert_eq!(cage.mkdir_syscall(path, S_IRWXA), -(Errno::EEXIST as i32));
        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_fs_mkdir_invalid_modebits() {
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);
        let path = "/parentdir";
        let invalid_mode = 0o77777; // Invalid mode bits
        // Create a parent directory
        cage.mkdir_syscall(path, S_IRWXA);
        // Check for error when a directory is being created with invalid mode
        assert_eq!(cage.mkdir_syscall("/parentdir/dir", invalid_mode), -(Errno::EPERM as i32));
        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_fs_mkdir_success() {
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);
        let path = "/parentdir";
        // Create a parent directory
        cage.mkdir_syscall(path, S_IRWXA);

        // Get the stat data for the parent directory and check for inode link count to be 3 initially
        let mut statdata = StatData::default();
        assert_eq!(cage.stat_syscall(path, &mut statdata), 0);
        assert_eq!(statdata.st_nlink, 3);

        // Create a child directory inside parent directory with valid mode bits
        assert_eq!(cage.mkdir_syscall("/parentdir/dir", S_IRWXA), 0);
        
        // Get the stat data for the child directory and check for inode link count to be 3 initially
        let mut statdata2 = StatData::default();
        assert_eq!(cage.stat_syscall("/parentdir/dir", &mut statdata2), 0);
        assert_eq!(statdata2.st_nlink, 3);

        // Get the stat data for the parent directory and check for inode link count to be 4 now as a new child directory has been created.
        let mut statdata3 = StatData::default();
        assert_eq!(cage.stat_syscall(path, &mut statdata3), 0);
        assert_eq!(statdata3.st_nlink, 4);

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_fs_mkdir_using_symlink() {
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);

        // Create a file which will be referred to as originalFile 
        let fd = cage.open_syscall("/originalFile", O_CREAT | O_EXCL | O_WRONLY, S_IRWXA);
        assert_eq!(cage.write_syscall(fd, str2cbuf("hi"), 2), 2);
        
        // Create a link between two files where the symlinkFile is originally not present
        // But while linking, symlinkFile will get created
        assert_eq!(cage.link_syscall("/originalFile", "/symlinkFile"), 0);

        // Check for error while creating the symlinkFile again as it would already be created while linking the two files above.
        assert_eq!(cage.mkdir_syscall("/symlinkFile", S_IRWXA), -(Errno::EEXIST as i32));
        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_fs_open_empty_directory() {
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);
        let path = "";
        // Check for error when directory is empty
        assert_eq!(cage.open_syscall(path, O_CREAT | O_TRUNC | O_RDWR, S_IRWXA), -(Errno::ENOENT as i32));
        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_fs_open_nonexisting_parentdirectory_and_file() {
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);
        let path = "/dir/file";
        // Check for error when neither file nor parent exists and O_CREAT flag is not present
        assert_eq!(cage.open_syscall(path, F_GETFD, S_IRWXA), -(Errno::ENOENT as i32));

        // Check for error when neither file nor parent exists and O_CREAT flag is present
        assert_eq!(cage.open_syscall(path, O_CREAT, S_IRWXA), -(Errno::ENOENT as i32));

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_fs_open_existing_parentdirectory_and_nonexisting_file() {
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);
        // Create a parent directory
        assert_eq!(cage.mkdir_syscall("/dir", S_IRWXA), 0);
        let path = "/dir/file";

        // Check for error when parent directory exists but file doesn't exist and O_CREAT is not present
        assert_eq!(cage.open_syscall(path, O_TRUNC, S_IRWXA), -(Errno::ENOENT as i32));

        // Check for error when parent directory exists but file doesn't exist and Filetype Flags contain S_IFCHR flag
        assert_eq!(cage.open_syscall(path, S_IFCHR | O_CREAT, S_IRWXA), -(Errno::EINVAL as i32));
        
        // Check for error when parent directory exists but file doesn't exist and mode bits are invalid
        let invalid_mode = 0o77777;
        assert_eq!(cage.open_syscall(path, O_CREAT, invalid_mode), -(Errno::EPERM as i32));

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_fs_open_existing_file_without_flags() {
        // This test is used for validating two scenarios:
        // 1. When the non-existing file is opened using O_CREAT flag, it should open successfully.
        // 2. When the same existing file is being opened without O_CREAT flag, it should open successfully.
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);
    
        // Open a non-existing file with O_CREAT flag
        // This should create a new file with a valid file descriptor
        let path = "/test";
        let fd = cage.open_syscall(path, O_CREAT | O_RDWR, S_IRWXA);
        assert!(fd > 0);
    
        // Open the existing file without O_CREAT and O_EXCL
        // The file should open successfully as the two flags are not set while re-opening the file
        let fd2 = cage.open_syscall(path, O_RDONLY, 0);
        assert!(fd2 > 0);
    
        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_fs_open_existing_file_with_flags() {
        // This test is used for validating two scenarios:
        // 1. When the non-existing file is opened using O_CREAT flag, it should open successfully.
        // 2. When the same existing file is opened using O_CREAT and O_EXCL flags, it should return an error for file already existing.
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);
    
        // Open a non-existing file with O_CREAT flag
        // This should create a new file with a valid file descriptor
        let path = "/test";
        let fd = cage.open_syscall(path, O_CREAT | O_RDWR, S_IRWXA);
        assert!(fd > 0);
    
        // Open the existing file with O_CREAT and O_EXCL flags
        // The file should not open successfully as the two flags are set while re-opening the file
        // It should return an error for "File already exists"
        assert_eq!(cage.open_syscall(path, O_CREAT | O_EXCL | O_RDONLY, S_IRWXA), -(Errno::EEXIST as i32));
    
        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_fs_open_create_new_file_and_check_link_count() {
        lindrustinit(0);
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
    
        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_fs_open_existing_file_with_o_trunc_flag() {
        lindrustinit(0);
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
    
        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_fs_open_new_file_with_s_ifchar_flag() {
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);

        // Create a parent directory
        assert_eq!(cage.mkdir_syscall("/testdir", S_IRWXA), 0);
        let path = "/testdir/file";

        // Attempt to open a file with S_IFCHR flag, which should be invalid for regular files
        assert_eq!(cage.open_syscall(path, O_CREAT | S_IFCHR, S_IRWXA), -(Errno::EINVAL as i32));
        
        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }
}
