#[cfg(test)]
pub mod ipc_tests {
    use super::super::*;
    use crate::interface;
    use crate::safeposix::{cage::*, dispatcher::*, filesystem};
    use libc::c_void;
    use std::fs::OpenOptions;
    use std::os::unix::fs::PermissionsExt;
    use std::time::Instant;

    //#[test]
    pub fn test_ipc() {
        ut_lind_ipc_pipe();
        ut_lind_ipc_domain_socket();
        ut_lind_ipc_socketpair();
    }

    pub fn ut_lind_ipc_pipe() {
        let byte_chunk: usize = 131072; // 128 KB
        let num_writes: usize = 8192; // 8 KB

        lindrustinit(0);

        let cage1 = interface::cagetable_getref(1);

        let mut pipefds = PipeArray {
            readfd: -1,
            writefd: -1,
        };
        assert_eq!(cage1.pipe_syscall(&mut pipefds), 0);
        assert_eq!(cage1.fork_syscall(2), 0);

        let sender = std::thread::spawn(move || {
            let cage2 = interface::cagetable_getref(2);

            assert_eq!(cage2.close_syscall(pipefds.writefd), 0);
            assert_eq!(cage2.dup2_syscall(pipefds.readfd, 0), 0);
            assert_eq!(cage2.close_syscall(pipefds.readfd), 0);

            let mut bytes_read: usize = 1;

            let mut buf: Vec<u8> = Vec::with_capacity(byte_chunk * num_writes);
            let mut bufptr = buf.as_mut_ptr();
            let mut buflen: usize = 0;

            while bytes_read != 0 {
                bytes_read = cage2.read_syscall(0, bufptr, byte_chunk) as usize;
                unsafe {
                    bufptr = bufptr.add(bytes_read);
                }
                buf.resize(buflen + bytes_read, 0);
                buflen += bytes_read;
            }
            assert_eq!(cage2.close_syscall(0), 0);

            assert_eq!(cage2.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        });

        assert_eq!(cage1.close_syscall(pipefds.readfd), 0);
        assert_eq!(cage1.dup2_syscall(pipefds.writefd, 1), 1);
        assert_eq!(cage1.close_syscall(pipefds.writefd), 0);

        for _i in 0..num_writes {
            let mut buf: Vec<u8> = vec!['A' as u8; byte_chunk];
            let bufptr = buf.as_mut_ptr();
            buf.resize(byte_chunk, 0);
            cage1.write_syscall(1, bufptr, byte_chunk);
        }

        assert_eq!(cage1.close_syscall(1), 0);

        sender.join().unwrap();

        assert_eq!(cage1.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);

        lindrustfinalize();
    }

    pub fn ut_lind_ipc_domain_socket() {
        //bind net zero test reformatted for domain sockets

        let clientsockfilename = "/client.sock";
        let serversockfilename = "/server.sock";

        lindrustinit(0);
        let cage = interface::cagetable_getref(1);

        //both the server and the socket are run from this file
        let serversockfd = cage.socket_syscall(AF_UNIX, SOCK_STREAM, 0);
        let clientsockfd = cage.socket_syscall(AF_UNIX, SOCK_STREAM, 0);

        //making sure that the assigned fd's are valid
        assert!(serversockfd > 0);
        assert!(clientsockfd > 0);

        //binding to a socket
        let serversockaddr =
            interface::new_sockaddr_unix(AF_UNIX as u16, serversockfilename.as_bytes());
        let serversocket = interface::GenSockaddr::Unix(serversockaddr);
        let clientsockaddr =
            interface::new_sockaddr_unix(AF_UNIX as u16, clientsockfilename.as_bytes());
        let clientsocket = interface::GenSockaddr::Unix(clientsockaddr);

        assert_eq!(cage.bind_syscall(serversockfd, &serversocket), 0);
        assert_eq!(cage.bind_syscall(clientsockfd, &clientsocket), 0);
        assert_eq!(cage.listen_syscall(serversockfd, 1), 0); //we are only allowing for one client at a time

        //forking the cage to get another cage with the same information
        assert_eq!(cage.fork_syscall(2), 0);

        //creating a thread for the server so that the information can be sent between the two threads
        let thread = interface::helper_thread(move || {
            let cage2 = interface::cagetable_getref(2);
            let mut socket2 = interface::GenSockaddr::Unix(interface::new_sockaddr_unix(
                AF_UNIX as u16,
                "".as_bytes(),
            )); // blank unix sockaddr

            let sockfd = cage2.accept_syscall(serversockfd, &mut socket2); //really can only make sure that the fd is valid
            assert!(sockfd > 0);

            interface::sleep(interface::RustDuration::from_millis(100));

            //process the first test...
            //Writing 100, then peek 100, then read 100
            let mut buf = sizecbuf(100);
            assert_eq!(
                cage2.recvfrom_syscall(
                    sockfd,
                    buf.as_mut_ptr(),
                    100,
                    MSG_PEEK,
                    &mut Some(&mut socket2)
                ),
                100
            ); //peeking at the input message
            assert_eq!(cbuf2str(&buf), &"A".repeat(100));
            buf = sizecbuf(100);
            assert_eq!(
                cage2.recvfrom_syscall(sockfd, buf.as_mut_ptr(), 100, 0, &mut Some(&mut socket2)),
                100
            ); //reading the input message
            assert_eq!(cbuf2str(&buf), &"A".repeat(100));
            buf = sizecbuf(100);

            interface::sleep(interface::RustDuration::from_millis(200));

            //process the second test...
            //Writing 100, read 20, peek 20, read 80
            assert_eq!(
                cage2.recvfrom_syscall(sockfd, buf.as_mut_ptr(), 20, 0, &mut Some(&mut socket2)),
                20
            );
            assert_eq!(cbuf2str(&buf), "A".repeat(20) + &"\0".repeat(80));
            buf = sizecbuf(100);
            assert_eq!(
                cage2.recvfrom_syscall(
                    sockfd,
                    buf.as_mut_ptr(),
                    20,
                    MSG_PEEK,
                    &mut Some(&mut socket2)
                ),
                20
            );
            assert_eq!(cbuf2str(&buf), "A".repeat(20) + &"\0".repeat(80));
            buf = sizecbuf(100);
            assert_eq!(
                cage2.recvfrom_syscall(sockfd, buf.as_mut_ptr(), 80, 0, &mut Some(&mut socket2)),
                80
            );
            assert_eq!(cbuf2str(&buf), "A".repeat(80) + &"\0".repeat(20));
            buf = sizecbuf(100);

            interface::sleep(interface::RustDuration::from_millis(200));

            //process the third test...
            //Writing 100, peek several times, read 100
            for _ in 0..4 {
                assert_eq!(
                    cage2.recvfrom_syscall(
                        sockfd,
                        buf.as_mut_ptr(),
                        10,
                        MSG_PEEK,
                        &mut Some(&mut socket2)
                    ),
                    10
                );
                assert_eq!(cbuf2str(&buf), "A".repeat(10) + &"\0".repeat(90));
                buf = sizecbuf(100);
            }
            for _ in 0..4 {
                assert_eq!(
                    cage2.recvfrom_syscall(
                        sockfd,
                        buf.as_mut_ptr(),
                        20,
                        MSG_PEEK,
                        &mut Some(&mut socket2)
                    ),
                    20
                );
                assert_eq!(cbuf2str(&buf), "A".repeat(20) + &"\0".repeat(80));
                buf = sizecbuf(100);
            }
            for _ in 0..4 {
                assert_eq!(
                    cage2.recvfrom_syscall(
                        sockfd,
                        buf.as_mut_ptr(),
                        30,
                        MSG_PEEK,
                        &mut Some(&mut socket2)
                    ),
                    30
                );
                assert_eq!(cbuf2str(&buf), "A".repeat(30) + &"\0".repeat(70));
                buf = sizecbuf(100);
            }
            for _ in 0..4 {
                assert_eq!(
                    cage2.recvfrom_syscall(
                        sockfd,
                        buf.as_mut_ptr(),
                        40,
                        MSG_PEEK,
                        &mut Some(&mut socket2)
                    ),
                    40
                );
                assert_eq!(cbuf2str(&buf), "A".repeat(40) + &"\0".repeat(60));
                buf = sizecbuf(100);
            }
            assert_eq!(
                cage2.recvfrom_syscall(sockfd, buf.as_mut_ptr(), 100, 0, &mut Some(&mut socket2)),
                100
            );
            assert_eq!(cbuf2str(&buf), &"A".repeat(100));
            buf = sizecbuf(100);

            interface::sleep(interface::RustDuration::from_millis(200));

            //process the fourth test...
            //Writing 50, peek 50
            assert_eq!(
                cage2.recvfrom_syscall(
                    sockfd,
                    buf.as_mut_ptr(),
                    50,
                    MSG_PEEK,
                    &mut Some(&mut socket2)
                ),
                50
            );
            assert_eq!(cbuf2str(&buf), "A".repeat(50) + &"\0".repeat(50));
            assert_eq!(cage2.close_syscall(sockfd), 0);
            assert_eq!(cage2.close_syscall(serversockfd), 0);

            assert_eq!(cage2.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        });

        //connect to the server
        interface::sleep(interface::RustDuration::from_millis(20));

        assert_eq!(cage.connect_syscall(clientsockfd, &serversocket), 0);

        //send the data with delays so that the server can process the information cleanly
        assert_eq!(
            cage.send_syscall(clientsockfd, str2cbuf(&"A".repeat(100)), 100, 0),
            100
        );
        interface::sleep(interface::RustDuration::from_millis(100));

        assert_eq!(
            cage.send_syscall(clientsockfd, str2cbuf(&"A".repeat(100)), 100, 0),
            100
        );
        interface::sleep(interface::RustDuration::from_millis(100));

        assert_eq!(
            cage.send_syscall(clientsockfd, str2cbuf(&"A".repeat(100)), 100, 0),
            100
        );
        interface::sleep(interface::RustDuration::from_millis(100));

        assert_eq!(
            cage.send_syscall(clientsockfd, str2cbuf(&"A".repeat(50)), 50, 0),
            50
        );
        interface::sleep(interface::RustDuration::from_millis(100));

        assert_eq!(cage.close_syscall(clientsockfd), 0);

        thread.join().unwrap();

        cage.unlink_syscall(serversockfilename);
        cage.unlink_syscall(clientsockfilename);

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_ipc_socketpair() {
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);
        let mut socketpair = interface::SockPair::default();
        assert_eq!(
            Cage::socketpair_syscall(cage.clone(), AF_UNIX, SOCK_STREAM, 0, &mut socketpair),
            0
        );
        let cage2 = cage.clone();

        let thread = interface::helper_thread(move || {
            let mut buf = sizecbuf(10);
            cage2.recv_syscall(socketpair.sock2, buf.as_mut_ptr(), 10, 0);
            assert_eq!(cbuf2str(&buf), "test\0\0\0\0\0\0");

            interface::sleep(interface::RustDuration::from_millis(30));
            assert_eq!(
                cage2.send_syscall(socketpair.sock2, str2cbuf("Socketpair Test"), 15, 0),
                15
            );
        });

        assert_eq!(
            cage.send_syscall(socketpair.sock1, str2cbuf("test"), 4, 0),
            4
        );

        let mut buf2 = sizecbuf(15);
        cage.recv_syscall(socketpair.sock1, buf2.as_mut_ptr(), 15, 0);
        assert_eq!(cbuf2str(&buf2), "Socketpair Test");

        thread.join().unwrap();

        assert_eq!(cage.close_syscall(socketpair.sock1), 0);
        assert_eq!(cage.close_syscall(socketpair.sock2), 0);

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_ipc_writev() {
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);
        let mut socketpair = interface::SockPair::default();
        assert_eq!(
            Cage::socketpair_syscall(cage.clone(), AF_UNIX, SOCK_STREAM, 0, &mut socketpair),
            0
        );
        let cage2 = cage.clone();

        let thread = interface::helper_thread(move || {
            let mut buf = sizecbuf(10);
            cage2.recv_syscall(socketpair.sock2, buf.as_mut_ptr(), 10, 0);
            assert_eq!(cbuf2str(&buf), "test\0\0\0\0\0\0");

            interface::sleep(interface::RustDuration::from_millis(30));

            let iovec: [interface::IovecStruct; 3] = [
                interface::IovecStruct {
                    iov_base: str2cbuf(&"A".repeat(100)) as *mut c_void,
                    iov_len: 100,
                },
                interface::IovecStruct {
                    iov_base: str2cbuf(&"B".repeat(100)) as *mut c_void,
                    iov_len: 100,
                },
                interface::IovecStruct {
                    iov_base: str2cbuf(&"C".repeat(100)) as *mut c_void,
                    iov_len: 100,
                },
            ];

            assert_eq!(
                cage2.writev_syscall(socketpair.sock2, iovec.as_ptr(), 3),
                300
            );
        });

        let iovec2: [interface::IovecStruct; 1] = [interface::IovecStruct {
            iov_base: str2cbuf("test") as *mut c_void,
            iov_len: 4,
        }];
        assert_eq!(cage.writev_syscall(socketpair.sock1, iovec2.as_ptr(), 1), 4);

        let mut buf2 = sizecbuf(300);
        assert_eq!(
            cage.recv_syscall(socketpair.sock1, buf2.as_mut_ptr(), 300, 0),
            300
        );
        thread.join().unwrap();

        assert_eq!(cage.close_syscall(socketpair.sock1), 0);
        assert_eq!(cage.close_syscall(socketpair.sock2), 0);

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }
}
