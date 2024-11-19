#[cfg(test)]
pub mod net_tests {
    use super::super::*;
    use crate::interface;
    use crate::safeposix::{cage::*, dispatcher::*, filesystem};
    use libc::c_void;
    use std::mem::size_of;
    use std::sync::{Arc, Barrier};

    #[test]
    pub fn ut_lind_net_bind() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);
        let sockfd = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        let port: u16 = generate_random_port();
        let socket = interface::GenSockaddr::V4(interface::SockaddrV4 {
            sin_family: AF_INET as u16,
            sin_port: port.to_be(),
            sin_addr: interface::V4Addr {
                s_addr: u32::from_ne_bytes([127, 0, 0, 1]),
            },
            padding: 0,
        }); //127.0.0.1

        //first bind should work... but second bind should not
        assert_eq!(cage.bind_syscall(sockfd, &socket), 0);
        assert_eq!(cage.bind_syscall(sockfd, &socket), -(Errno::EINVAL as i32)); //already bound so should fail

        //trying to bind another to the same IP/PORT
        let sockfd2 = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        assert_eq!(
            cage.bind_syscall(sockfd2, &socket),
            -(Errno::EADDRINUSE as i32)
        ); //already bound so should fail

        //UDP should still work...
        let sockfd3 = cage.socket_syscall(AF_INET, SOCK_DGRAM, 0);
        assert_eq!(cage.bind_syscall(sockfd3, &socket), 0);

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_net_bind_on_zero() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        //both the server and the socket are run from this file
        let serversockfd = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        let clientsockfd = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        let clientsockfd2 = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        //making sure that the assigned fd's are valid
        assert!(serversockfd > 0);
        assert!(clientsockfd > 0);
        assert!(clientsockfd2 > 0);
        let port: u16 = generate_random_port();
        //binding to a socket
        let sockaddr = interface::SockaddrV4 {
            sin_family: AF_INET as u16,
            sin_port: port.to_be(),
            sin_addr: interface::V4Addr {
                s_addr: u32::from_ne_bytes([127, 0, 0, 1]),
            },
            padding: 0,
        };
        let socket = interface::GenSockaddr::V4(sockaddr); //127.0.0.1
        assert_eq!(cage.bind_syscall(serversockfd, &socket), 0);
        assert_eq!(cage.listen_syscall(serversockfd, 1), 0); //we are only allowing for one client at a time

        //forking the cage to get another cage with the same information
        assert_eq!(cage.fork_syscall(2), 0);

        //creating a thread for the server so that the information can be sent between
        // the two threads
        let thread = interface::helper_thread(move || {
            let cage2 = interface::cagetable_getref(2);
            let port: u16 = generate_random_port();
            let mut socket2 = interface::GenSockaddr::V4(interface::SockaddrV4 {
                sin_family: AF_INET as u16,
                sin_port: port.to_be(),
                sin_addr: interface::V4Addr { s_addr: 0 },
                padding: 0,
            }); //0.0.0.0

            let mut sockfd = cage2.accept_syscall(serversockfd, &mut socket2); //really can only make sure that the fd is valid
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
            let port: u16 = generate_random_port();
            socket2 = interface::GenSockaddr::V4(interface::SockaddrV4 {
                sin_family: AF_INET as u16,
                sin_port: port.to_be(),
                sin_addr: interface::V4Addr { s_addr: 0 },
                padding: 0,
            }); //0.0.0.0
            interface::sleep(interface::RustDuration::from_millis(200));
            sockfd = cage2.accept_syscall(serversockfd, &mut socket2); //really can only make sure that the fd is valid
            assert!(sockfd > 0);

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

            interface::sleep(interface::RustDuration::from_millis(200));

            //process the third test...
            //Writing 100, peek several times, read 100
            for _ in 0..4 {
                buf = sizecbuf(100);
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
            }
            for _ in 0..4 {
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
            }
            for _ in 0..4 {
                buf = sizecbuf(100);
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
            }
            for _ in 0..4 {
                buf = sizecbuf(100);
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
            }
            buf = sizecbuf(100);
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

            interface::sleep(interface::RustDuration::from_millis(100));

            assert_eq!(cage2.close_syscall(sockfd), 0);
            assert_eq!(cage2.close_syscall(serversockfd), 0);

            assert_eq!(cage2.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        });

        //connect to the server
        interface::sleep(interface::RustDuration::from_millis(20));

        assert_eq!(cage.connect_syscall(clientsockfd, &socket), 0);

        //send the data with delays so that the server can process the information
        // cleanly
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

        //connect to the server with the other sockfd
        assert_eq!(cage.connect_syscall(clientsockfd2, &socket), 0);

        //send the data with delays so that the server can process the information
        // cleanly
        assert_eq!(
            cage.send_syscall(clientsockfd2, str2cbuf(&"A".repeat(100)), 100, 0),
            100
        );
        interface::sleep(interface::RustDuration::from_millis(100));

        assert_eq!(
            cage.send_syscall(clientsockfd2, str2cbuf(&"A".repeat(100)), 100, 0),
            100
        );
        interface::sleep(interface::RustDuration::from_millis(100));

        assert_eq!(
            cage.send_syscall(clientsockfd2, str2cbuf(&"A".repeat(100)), 100, 0),
            100
        );
        interface::sleep(interface::RustDuration::from_millis(100));

        assert_eq!(
            cage.send_syscall(clientsockfd2, str2cbuf(&"A".repeat(50)), 50, 0),
            50
        );
        interface::sleep(interface::RustDuration::from_millis(100));

        assert_eq!(cage.close_syscall(clientsockfd2), 0);

        thread.join().unwrap();

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_net_bind_multiple() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        let mut sockfd = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        let port: u16 = generate_random_port();
        let socket = interface::GenSockaddr::V4(interface::SockaddrV4 {
            sin_family: AF_INET as u16,
            sin_port: port.to_be(),
            sin_addr: interface::V4Addr {
                s_addr: u32::from_ne_bytes([127, 0, 0, 1]),
            },
            padding: 0,
        }); //127.0.0.1
        assert_eq!(
            cage.setsockopt_syscall(sockfd, SOL_SOCKET, SO_REUSEPORT, 1),
            0
        );
        assert_eq!(cage.bind_syscall(sockfd, &socket), 0);

        let sockfd2 = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);

        //allowing port reuse
        assert_eq!(
            cage.setsockopt_syscall(sockfd2, SOL_SOCKET, SO_REUSEPORT, 1),
            0
        );

        assert_eq!(cage.bind_syscall(sockfd2, &socket), 0);

        //double listen should be allowed
        assert_eq!(cage.listen_syscall(sockfd, 1), 0);
        assert_eq!(cage.listen_syscall(sockfd2, 1), 0);

        //UDP bind should be allowed
        sockfd = cage.socket_syscall(AF_INET, SOCK_DGRAM, 0);
        assert_eq!(cage.bind_syscall(sockfd, &socket), 0);

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_net_connect_basic_udp() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        //should be okay...
        let sockfd = cage.socket_syscall(AF_INET, SOCK_DGRAM, 0);
        let port: u16 = generate_random_port();
        let mut socket = interface::GenSockaddr::V4(interface::SockaddrV4 {
            sin_family: AF_INET as u16,
            sin_port: port.to_be(),
            sin_addr: interface::V4Addr {
                s_addr: u32::from_ne_bytes([127, 0, 0, 1]),
            },
            padding: 0,
        }); //127.0.0.1
        assert_eq!(cage.connect_syscall(sockfd, &socket), 0);
        let port: u16 = generate_random_port();
        //should be able to retarget the socket
        socket = interface::GenSockaddr::V4(interface::SockaddrV4 {
            sin_family: AF_INET as u16,
            sin_port: port.to_be(),
            sin_addr: interface::V4Addr {
                s_addr: u32::from_ne_bytes([127, 0, 0, 1]),
            },
            padding: 0,
        }); //127.0.0.1
        assert_eq!(cage.connect_syscall(sockfd, &socket), 0);

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    #[ignore]
    //Test connect sys call using AF_INET6/IPv6 address family and UDP socket type
    //Currently failing as IPv6 is not implemented via gen_netdevs
    pub fn ut_lind_net_connect_basic_udp_ipv6() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        //Initialize initial socket fd and remote socket to connect to
        let sockfd = cage.socket_syscall(AF_INET6, SOCK_DGRAM, 0);
        let port: u16 = generate_random_port();
        let mut socket = interface::GenSockaddr::V6(interface::SockaddrV6 {
            sin6_family: AF_INET6 as u16,
            sin6_port: port.to_be(),
            sin6_addr: interface::V6Addr {
                s6_addr: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1],
            },
            sin6_flowinfo: 0,
            sin6_scope_id: 0,
        }); //::1 LOCALHOST
        assert_eq!(cage.connect_syscall(sockfd, &socket), 0);

        //Change the port and retarget the socket
        let port: u16 = generate_random_port();
        socket = interface::GenSockaddr::V6(interface::SockaddrV6 {
            sin6_family: AF_INET6 as u16,
            sin6_port: port.to_be(),
            sin6_addr: interface::V6Addr {
                s6_addr: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1],
            },
            sin6_flowinfo: 0,
            sin6_scope_id: 0,
        }); //::1 LOCALHOST
        assert_eq!(cage.connect_syscall(sockfd, &socket), 0);

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_net_getpeername_bad_input() {
        // this test is used for testing getpeername with invalid input

        // acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        let filefd = cage.open_syscall("/getpeernametest.txt", O_CREAT | O_EXCL | O_RDWR, S_IRWXA);
        assert!(filefd > 0);

        // test for invalid file descriptor
        let mut retsocket = interface::GenSockaddr::V4(interface::SockaddrV4::default()); //127.0.0.1
                                                                                          // fd that is out of range
        assert_eq!(
            cage.getpeername_syscall(-1, &mut retsocket),
            -(Errno::EBADF as i32)
        );
        // fd that does not exist
        assert_eq!(
            cage.getpeername_syscall(10, &mut retsocket),
            -(Errno::EBADF as i32)
        );
        // fd that is not socket
        assert_eq!(
            cage.getpeername_syscall(filefd, &mut retsocket),
            -(Errno::ENOTSOCK as i32)
        );

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_net_getpeername_inet() {
        // this test is used for testing getpeername on AF_INET sockets

        // acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        let sockfd = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        let clientsockfd1 = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);

        let port: u16 = generate_random_port();
        let socket = interface::GenSockaddr::V4(interface::SockaddrV4 {
            sin_family: AF_INET as u16,
            sin_port: port.to_be(),
            sin_addr: interface::V4Addr {
                s_addr: u32::from_ne_bytes([127, 0, 0, 1]),
            },
            padding: 0,
        }); // 127.0.0.1

        let mut retsocket = interface::GenSockaddr::V4(interface::SockaddrV4::default());
        assert_eq!(cage.bind_syscall(sockfd, &socket), 0);
        // we havn't connected yet, so it should return ENOTCONN error
        assert_eq!(
            cage.getpeername_syscall(sockfd, &mut retsocket),
            -(Errno::ENOTCONN as i32)
        );

        assert_eq!(cage.fork_syscall(2), 0); // used for AF_INET thread client

        let barrier = Arc::new(Barrier::new(2));
        let barrier_clone = barrier.clone();

        let threadclient1 = interface::helper_thread(move || {
            let cage2 = interface::cagetable_getref(2);
            assert_eq!(cage2.close_syscall(sockfd), 0);

            barrier_clone.wait();
            // connect to server
            assert_eq!(cage2.connect_syscall(clientsockfd1, &socket), 0);

            let mut retsocket = interface::GenSockaddr::V4(interface::SockaddrV4::default());
            assert_eq!(cage2.getpeername_syscall(clientsockfd1, &mut retsocket), 0);
            // the retsocket should be exactly the same as the address of the server
            assert_eq!(retsocket, socket);

            cage2.exit_syscall(EXIT_SUCCESS);
        });

        assert_eq!(cage.listen_syscall(sockfd, 4), 0);
        let mut sockgarbage = interface::GenSockaddr::V4(interface::SockaddrV4::default());
        barrier.wait();
        let fd = cage.accept_syscall(sockfd as i32, &mut sockgarbage);
        assert!(fd > 0);

        let mut retsocket = interface::GenSockaddr::V4(interface::SockaddrV4::default());
        assert_eq!(cage.getpeername_syscall(fd, &mut retsocket), 0);
        // peer's port should not be zero
        assert_ne!(retsocket.port(), 0);
        // peer's address should be 127.0.0.1
        assert_eq!(
            retsocket.addr(),
            interface::GenIpaddr::V4(interface::V4Addr {
                s_addr: u32::from_ne_bytes([127, 0, 0, 1]),
            })
        );

        threadclient1.join().unwrap();

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_net_getpeername_unix() {
        // this test is used for testing getpeername on AF_UNIX sockets

        // acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        let sockfd = cage.socket_syscall(AF_UNIX, SOCK_STREAM, 0);

        let serversockaddr_unix =
            interface::new_sockaddr_unix(AF_UNIX as u16, "server_getpeername".as_bytes());
        let serversocket_unix = interface::GenSockaddr::Unix(serversockaddr_unix);
        assert_eq!(cage.bind_syscall(sockfd, &serversocket_unix), 0);

        assert_eq!(cage.fork_syscall(2), 0);

        let barrier = Arc::new(Barrier::new(2));
        let barrier_clone = barrier.clone();

        let threadclient1 = interface::helper_thread(move || {
            let cage2 = interface::cagetable_getref(2);
            assert_eq!(cage2.close_syscall(sockfd), 0);

            let clientsockfd1 = cage2.socket_syscall(AF_UNIX, SOCK_STREAM, 0);

            barrier_clone.wait();
            // connect to server
            assert_eq!(cage2.connect_syscall(clientsockfd1, &serversocket_unix), 0);

            let mut retsocket = interface::GenSockaddr::Unix(interface::new_sockaddr_unix(0, &[]));
            assert_eq!(cage2.getpeername_syscall(clientsockfd1, &mut retsocket), 0);
            let unixinfo = match retsocket {
                interface::GenSockaddr::Unix(value) => value,
                _ => unreachable!(),
            };
            // client's peer address should be exactly same as the server address
            assert_eq!(unixinfo, serversockaddr_unix);

            cage2.exit_syscall(EXIT_SUCCESS);
        });

        assert_eq!(cage.listen_syscall(sockfd, 1), 0);
        let mut sockgarbage = interface::GenSockaddr::Unix(interface::new_sockaddr_unix(
            AF_UNIX as u16,
            "".as_bytes(),
        ));
        barrier.wait();
        let fd = cage.accept_syscall(sockfd as i32, &mut sockgarbage);
        assert!(fd > 0);

        let mut retsocket = interface::GenSockaddr::Unix(interface::new_sockaddr_unix(0, &[]));
        assert_eq!(cage.getpeername_syscall(fd, &mut retsocket), 0);
        let unixinfo = match retsocket {
            interface::GenSockaddr::Unix(value) => value,
            _ => unreachable!(),
        };
        // server's peer address should have family of AF_UNIX
        assert_eq!(unixinfo.sun_family, AF_UNIX as u16);
        // and a path that is not empty
        assert_ne!(unixinfo.sun_path[0], 0);

        threadclient1.join().unwrap();

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_net_getpeername_udp() {
        // acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        //doing a few things with connect -- only UDP right now
        let sockfd = cage.socket_syscall(AF_INET, SOCK_DGRAM, 0);
        let port: u16 = generate_random_port();
        let mut socket = interface::GenSockaddr::V4(interface::SockaddrV4 {
            sin_family: AF_INET as u16,
            sin_port: port.to_be(),
            sin_addr: interface::V4Addr {
                s_addr: u32::from_ne_bytes([127, 0, 0, 1]),
            },
            padding: 0,
        }); //127.0.0.1
        let mut retsocket = interface::GenSockaddr::V4(interface::SockaddrV4::default()); //127.0.0.1

        assert_eq!(cage.connect_syscall(sockfd, &socket), 0);
        assert_eq!(cage.getpeername_syscall(sockfd, &mut retsocket), 0);
        assert_eq!(retsocket, socket);
        let port: u16 = generate_random_port();
        //should be able to retarget
        socket = interface::GenSockaddr::V4(interface::SockaddrV4 {
            sin_family: AF_INET as u16,
            sin_port: port.to_be(),
            sin_addr: interface::V4Addr {
                s_addr: u32::from_ne_bytes([127, 0, 0, 1]),
            },
            padding: 0,
        }); //127.0.0.1
        assert_eq!(cage.connect_syscall(sockfd, &socket), 0);
        assert_eq!(cage.getpeername_syscall(sockfd, &mut retsocket), 0);
        assert_eq!(retsocket, socket);

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_net_getsockname_bad_input() {
        // this test is used for testing getsockname with invalid input

        // acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        // test for passing invalid fd
        let mut retsocket = interface::GenSockaddr::V4(interface::SockaddrV4::default());
        assert_eq!(
            cage.getsockname_syscall(10, &mut retsocket),
            -(Errno::EBADF as i32)
        );

        // test for passing fd that is not socket
        let filefd = cage.open_syscall("/getsocknametest.txt", O_CREAT | O_EXCL | O_RDWR, S_IRWXA);
        assert!(filefd > 0);
        let mut retsocket = interface::GenSockaddr::V4(interface::SockaddrV4::default());
        assert_eq!(
            cage.getsockname_syscall(filefd, &mut retsocket),
            -(Errno::ENOTSOCK as i32)
        );

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_net_getsockname_empty() {
        // this test is used for testing getsockname with socket that hasn't bound to
        // any address

        // acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        // test for ipv4
        let emptysocket_ipv4 = interface::GenSockaddr::V4(interface::SockaddrV4 {
            sin_family: AF_INET as u16,
            sin_port: 0,
            sin_addr: interface::V4Addr::default(),
            padding: 0,
        });

        let sockfd = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        assert!(sockfd >= 0);
        let mut retsocket = interface::GenSockaddr::V4(interface::SockaddrV4::default());
        assert_eq!(cage.getsockname_syscall(sockfd, &mut retsocket), 0);
        assert_eq!(retsocket, emptysocket_ipv4);

        // test for unix socket
        let sockfd = cage.socket_syscall(AF_UNIX, SOCK_STREAM, 0);
        assert!(sockfd >= 0);
        let mut retsocket =
            interface::GenSockaddr::Unix(interface::new_sockaddr_unix(SOCK_STREAM as u16, &[]));
        let emptysocket_unix =
            interface::GenSockaddr::Unix(interface::new_sockaddr_unix(SOCK_STREAM as u16, &[]));
        assert_eq!(cage.getsockname_syscall(sockfd, &mut retsocket), 0);
        assert_eq!(retsocket, emptysocket_unix);

        // test for ipv6
        let sockfd = cage.socket_syscall(AF_INET6, SOCK_STREAM, 0);
        assert!(sockfd >= 0);
        let mut retsocket = interface::GenSockaddr::V6(interface::SockaddrV6::default());
        assert_eq!(cage.getsockname_syscall(sockfd, &mut retsocket), 0);
        // port should be 0
        assert_eq!(retsocket.port(), 0);
        // address should be ::
        assert_eq!(
            retsocket.addr(),
            interface::GenIpaddr::V6(interface::V6Addr::default())
        );

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    #[ignore]
    pub fn ut_lind_net_getsockname_inet_connect() {
        // temporary test derived from ut_lind_net_getsockname_inet due to a bug
        // once the bug is fixed, ideally should merge back into
        // ut_lind_net_getsockname_inet

        // acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        let sockfd = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        let clientsockfd1 = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);

        let port: u16 = generate_random_port();
        let socket = interface::GenSockaddr::V4(interface::SockaddrV4 {
            sin_family: AF_INET as u16,
            sin_port: port.to_be(),
            sin_addr: interface::V4Addr {
                s_addr: u32::from_ne_bytes([127, 0, 0, 1]),
            },
            padding: 0,
        }); // 127.0.0.1

        assert_eq!(cage.bind_syscall(sockfd, &socket), 0);

        assert_eq!(cage.fork_syscall(2), 0); // used for AF_INET thread client

        let barrier = Arc::new(Barrier::new(2));
        let barrier_clone = barrier.clone();

        let threadclient1 = interface::helper_thread(move || {
            let cage2 = interface::cagetable_getref(2);
            assert_eq!(cage2.close_syscall(sockfd), 0);

            barrier_clone.wait();
            // connect to server
            assert_eq!(cage2.connect_syscall(clientsockfd1, &socket), 0);

            let mut retsocket = interface::GenSockaddr::V4(interface::SockaddrV4::default());
            assert_eq!(cage2.getsockname_syscall(clientsockfd1, &mut retsocket), 0);

            // BUG: when calling connect without binding to any address, an new address will
            // be automatically assigned to the socket, and its ip address is supposed to be
            // 127.0.0.1, and port is not supposed to be 0. But we failed this test
            assert_ne!(retsocket.port(), 0);
            assert_eq!(
                retsocket.addr(),
                interface::GenIpaddr::V4(interface::V4Addr {
                    s_addr: u32::from_ne_bytes([127, 0, 0, 1]),
                })
            );

            cage2.exit_syscall(EXIT_SUCCESS);
        });

        assert_eq!(cage.listen_syscall(sockfd, 4), 0);
        let mut sockgarbage = interface::GenSockaddr::V4(interface::SockaddrV4::default());
        barrier.wait();
        let fd = cage.accept_syscall(sockfd as i32, &mut sockgarbage);
        assert!(fd > 0);

        threadclient1.join().unwrap();

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_net_getsockname_inet() {
        // this test is used for testing getsockname on AF_INET sockets

        // acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        let sockfd = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        let clientsockfd1 = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);

        let port: u16 = generate_random_port();
        let socket = interface::GenSockaddr::V4(interface::SockaddrV4 {
            sin_family: AF_INET as u16,
            sin_port: port.to_be(),
            sin_addr: interface::V4Addr {
                s_addr: u32::from_ne_bytes([127, 0, 0, 1]),
            },
            padding: 0,
        }); // 127.0.0.1

        let mut retsocket = interface::GenSockaddr::V4(interface::SockaddrV4::default());
        assert_eq!(cage.bind_syscall(sockfd, &socket), 0);
        assert_eq!(cage.getsockname_syscall(sockfd, &mut retsocket), 0);
        assert_eq!(retsocket, socket);

        // checking that we cannot rebind the socket
        assert_eq!(cage.bind_syscall(sockfd, &socket), -(Errno::EINVAL as i32)); //already bound so should fail
        assert_eq!(cage.getsockname_syscall(sockfd, &mut retsocket), 0);
        assert_eq!(retsocket, socket);

        assert_eq!(cage.fork_syscall(2), 0); // used for AF_INET thread client

        let barrier = Arc::new(Barrier::new(2));
        let barrier_clone = barrier.clone();

        let threadclient1 = interface::helper_thread(move || {
            let cage2 = interface::cagetable_getref(2);
            assert_eq!(cage2.close_syscall(sockfd), 0);

            barrier_clone.wait();
            // connect to server
            assert_eq!(cage2.connect_syscall(clientsockfd1, &socket), 0);

            let mut retsocket = interface::GenSockaddr::V4(interface::SockaddrV4::default());
            assert_eq!(cage2.getsockname_syscall(clientsockfd1, &mut retsocket), 0);

            // assert_ne!(retsocket.port(), 0);
            // assert_eq!(retsocket.addr(), interface::GenIpaddr::V4(interface::V4Addr {
            //     s_addr: u32::from_ne_bytes([127, 0, 0, 1]),
            // }));

            cage2.exit_syscall(EXIT_SUCCESS);
        });

        assert_eq!(cage.listen_syscall(sockfd, 4), 0);
        let mut sockgarbage = interface::GenSockaddr::V4(interface::SockaddrV4::default());
        barrier.wait();
        let fd = cage.accept_syscall(sockfd as i32, &mut sockgarbage);

        let mut retsocket = interface::GenSockaddr::V4(interface::SockaddrV4::default());
        assert_eq!(cage.getsockname_syscall(fd, &mut retsocket), 0);
        assert_ne!(retsocket.port(), 0);
        assert_eq!(
            retsocket.addr(),
            interface::GenIpaddr::V4(interface::V4Addr {
                s_addr: u32::from_ne_bytes([127, 0, 0, 1]),
            })
        );

        threadclient1.join().unwrap();

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_net_getsockname_unix() {
        // this test is used for testing getsockname on AF_UNIX sockets

        // acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        // create a AF_UNIX socket
        let sockfd = cage.socket_syscall(AF_UNIX, SOCK_STREAM, 0);

        let serversockaddr_unix =
            interface::new_sockaddr_unix(AF_UNIX as u16, "server_getsockname".as_bytes());
        let serversocket_unix = interface::GenSockaddr::Unix(serversockaddr_unix);
        assert_eq!(cage.bind_syscall(sockfd, &serversocket_unix), 0);

        // now the socket is bound to an address, check if the address returned from
        // getsockname_syscall is correct
        let mut retsocket = interface::GenSockaddr::Unix(interface::new_sockaddr_unix(0, &[]));
        assert_eq!(cage.getsockname_syscall(sockfd, &mut retsocket), 0);
        // retrieve the unix info
        let unixinfo = match retsocket {
            interface::GenSockaddr::Unix(value) => value,
            _ => unreachable!(),
        };
        assert_eq!(serversockaddr_unix, unixinfo);

        assert_eq!(cage.fork_syscall(2), 0);

        // this barrier is to coordinate server and client
        let barrier = Arc::new(Barrier::new(2));
        let barrier_clone = barrier.clone();

        let threadclient1 = interface::helper_thread(move || {
            // client thread
            let cage2 = interface::cagetable_getref(2);
            assert_eq!(cage2.close_syscall(sockfd), 0);

            let clientsockfd1 = cage2.socket_syscall(AF_UNIX, SOCK_STREAM, 0);

            barrier_clone.wait();
            // connect to server
            assert_eq!(cage2.connect_syscall(clientsockfd1, &serversocket_unix), 0);

            // the client address hasn't bound to an address before connect
            // so a new address must be automatically assigned to it
            let mut retsocket = interface::GenSockaddr::Unix(interface::new_sockaddr_unix(0, &[]));
            assert_eq!(cage2.getsockname_syscall(clientsockfd1, &mut retsocket), 0);
            // retrieve the unix info
            let unixinfo = match retsocket {
                interface::GenSockaddr::Unix(value) => value,
                _ => unreachable!(),
            };
            // check the family
            assert_eq!(unixinfo.sun_family, AF_UNIX as u16);
            // its path should not be empty
            // since we do not know the assigned address exactly
            // so we just check if the first character of the address it not null
            // to verify if the address is not empty
            assert_ne!(unixinfo.sun_path[0], 0);

            cage2.exit_syscall(EXIT_SUCCESS);
        });

        // server listen
        assert_eq!(cage.listen_syscall(sockfd, 1), 0);
        let mut sockgarbage = interface::GenSockaddr::Unix(interface::new_sockaddr_unix(
            AF_UNIX as u16,
            "".as_bytes(),
        ));
        barrier.wait();
        let fd = cage.accept_syscall(sockfd as i32, &mut sockgarbage);

        // the socket created from accept should be automatically assigned an address as
        // well
        let mut retsocket = interface::GenSockaddr::Unix(interface::new_sockaddr_unix(0, &[]));
        assert_eq!(cage.getsockname_syscall(fd, &mut retsocket), 0);
        let unixinfo = match retsocket {
            interface::GenSockaddr::Unix(value) => value,
            _ => unreachable!(),
        };
        // same check as client
        assert_eq!(unixinfo.sun_family, AF_UNIX as u16);
        assert_ne!(unixinfo.sun_path[0], 0);

        threadclient1.join().unwrap();

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_net_listen() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        let serversockfd = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        let clientsockfd = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);

        assert!(serversockfd > 0);
        assert!(clientsockfd > 0);
        let port: u16 = generate_random_port();
        //binding to a socket
        let sockaddr = interface::SockaddrV4 {
            sin_family: AF_INET as u16,
            sin_port: port.to_be(),
            sin_addr: interface::V4Addr {
                s_addr: u32::from_ne_bytes([127, 0, 0, 1]),
            },
            padding: 0,
        };
        let socket = interface::GenSockaddr::V4(sockaddr); //127.0.0.1
        assert_eq!(cage.bind_syscall(serversockfd, &socket), 0);
        assert_eq!(cage.listen_syscall(serversockfd, 10), 0);

        //forking the cage to get another cage with the same information
        assert_eq!(cage.fork_syscall(2), 0);

        let thread = interface::helper_thread(move || {
            let cage2 = interface::cagetable_getref(2);
            let mut socket2 = interface::GenSockaddr::V4(interface::SockaddrV4::default());
            assert!(cage2.accept_syscall(serversockfd, &mut socket2) > 0); //really can only make sure that the fd is valid

            assert_eq!(cage2.close_syscall(serversockfd), 0);
            assert_eq!(cage2.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        });

        interface::sleep(interface::RustDuration::from_millis(100));
        assert_eq!(cage.connect_syscall(clientsockfd, &socket), 0);

        let mut retsocket = interface::GenSockaddr::V4(interface::SockaddrV4::default());
        assert_eq!(cage.getsockname_syscall(clientsockfd, &mut retsocket), 0);
        assert_ne!(retsocket, socket);

        assert_eq!(cage.close_syscall(serversockfd), 0);

        thread.join().unwrap();

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    //Attempt to call accept on a socket that is not listening for a connection
    //Attempt to call accept on a socket that is closed
    //Both of these should return their respective errors, being EINVAL and EBADF
    pub fn ut_lind_net_accept_errs() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        let serversockfd = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        let clientsockfd = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);

        assert!(serversockfd > 0);
        assert!(clientsockfd > 0);
        let port: u16 = generate_random_port();
        //binding to a socket
        let sockaddr = interface::SockaddrV4 {
            sin_family: AF_INET as u16,
            sin_port: port.to_be(),
            sin_addr: interface::V4Addr {
                s_addr: u32::from_ne_bytes([127, 0, 0, 1]),
            },
            padding: 0,
        };
        let socket = interface::GenSockaddr::V4(sockaddr); //127.0.0.1
        assert_eq!(cage.bind_syscall(serversockfd, &socket), 0);

        //forking the cage to get another cage with the same information
        assert_eq!(cage.fork_syscall(2), 0);

        let thread = interface::helper_thread(move || {
            let cage2 = interface::cagetable_getref(2);
            let mut socket2 = interface::GenSockaddr::V4(interface::SockaddrV4::default());

            //Accept before listening
            assert_eq!(
                cage2.accept_syscall(serversockfd, &mut socket2),
                -(Errno::EINVAL as i32)
            );
            //Now listen
            assert_eq!(cage2.listen_syscall(serversockfd, 10), 0);

            //Check that fd > 0, indicating a valid connection
            assert!(cage2.accept_syscall(serversockfd, &mut socket2) > 0);
            //Close serverfd and check that it doesn't accept
            assert_eq!(cage2.close_syscall(serversockfd), 0);
            assert_eq!(
                cage2.accept_syscall(serversockfd, &mut socket2),
                -(Errno::EBADF as i32)
            );

            assert_eq!(cage2.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        });

        interface::sleep(interface::RustDuration::from_millis(1000));
        assert_eq!(cage.connect_syscall(clientsockfd, &socket), 0);

        let mut retsocket = interface::GenSockaddr::V4(interface::SockaddrV4::default());
        assert_eq!(cage.getsockname_syscall(clientsockfd, &mut retsocket), 0);
        assert_ne!(retsocket, socket);

        assert_eq!(cage.close_syscall(serversockfd), 0);

        thread.join().unwrap();

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    //We try to queue more than 1 connection with the
    //backlog is set to 1. We expect both connections to return with
    //errno set to EINPROGRESS as the sockets are non-blocking
    pub fn ut_lind_net_listen_more_than_backlog() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        let serversockfd = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        let clientsockfd1 = cage.socket_syscall(AF_INET, SOCK_STREAM | SOCK_NONBLOCK, 0);
        let clientsockfd2 = cage.socket_syscall(AF_INET, SOCK_STREAM | SOCK_NONBLOCK, 0);

        assert!(serversockfd > 0);
        assert!(clientsockfd1 > 0);
        assert!(clientsockfd2 > 0);
        let port: u16 = generate_random_port();
        //binding to a socket
        let sockaddr = interface::SockaddrV4 {
            sin_family: AF_INET as u16,
            sin_port: port.to_be(),
            sin_addr: interface::V4Addr {
                s_addr: u32::from_ne_bytes([127, 0, 0, 1]),
            },
            padding: 0,
        };
        let socket = interface::GenSockaddr::V4(sockaddr); //127.0.0.1
        assert_eq!(cage.bind_syscall(serversockfd, &socket), 0);
        assert_eq!(cage.listen_syscall(serversockfd, 1), 0);

        //forking the cage to get another cage with the same information
        assert_eq!(cage.fork_syscall(2), 0);

        let thread = interface::helper_thread(move || {
            let cage2 = interface::cagetable_getref(2);

            assert_eq!(
                cage2.connect_syscall(clientsockfd1, &socket),
                -(Errno::EINPROGRESS as i32)
            );

            interface::sleep(interface::RustDuration::from_millis(100));

            assert_eq!(
                cage2.connect_syscall(clientsockfd2, &socket),
                -(Errno::EINPROGRESS as i32)
            );

            assert_eq!(cage2.close_syscall(serversockfd), 0);
            assert_eq!(cage2.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        });

        assert_eq!(cage.close_syscall(serversockfd), 0);

        thread.join().unwrap();

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    //UDP Socket should not be able to listen
    //Fail with errno EOPNOTSUPP
    #[test]
    pub fn ut_lind_net_listen_udp_socket() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        let serversockfd = cage.socket_syscall(AF_INET, SOCK_DGRAM, 0);
        assert!(serversockfd > 0);
        let port: u16 = generate_random_port();
        //binding to a socket
        let sockaddr = interface::SockaddrV4 {
            sin_family: AF_INET as u16,
            sin_port: port.to_be(),
            sin_addr: interface::V4Addr {
                s_addr: u32::from_ne_bytes([127, 0, 0, 1]),
            },
            padding: 0,
        };
        let socket = interface::GenSockaddr::V4(sockaddr); //127.0.0.1
        assert_eq!(cage.bind_syscall(serversockfd, &socket), 0);
        assert_eq!(cage.listen_syscall(serversockfd, 10), -95); //why can't i use EOPNOTSUPP here??

        assert_eq!(cage.close_syscall(serversockfd), 0);

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_net_poll_bad_input() {
        // this test is used for testing poll with some error/edge cases

        // acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        // error case 1: invalid file descriptor
        // contruct a PollStruct with invalid fd (10)
        let mut polled = vec![interface::PollStruct {
            fd: 10,
            events: POLLIN,
            revents: 0,
        }];

        // exactly one fd should have non-zero revents field
        assert_eq!(cage.poll_syscall(&mut polled.as_mut_slice(), None), 1);
        // and its revents should be set to POLLNVAL
        assert_eq!(polled[0].revents, POLLNVAL);

        // error case 2: negative file descriptor should be ignored
        // contruct a PollStruct with negative fd
        let mut polled = vec![interface::PollStruct {
            fd: -1,
            events: POLLIN,
            revents: 0,
        }];

        // the fd should be ignored, so no error is expected
        assert_eq!(
            cage.poll_syscall(
                &mut polled.as_mut_slice(),
                Some(interface::RustDuration::ZERO)
            ),
            0
        );
        // revents should be 0
        assert_eq!(polled[0].revents, 0);

        // edge case: revents should always be cleared
        // create a file
        let filefd = cage.open_syscall("/netpolltest.txt", O_CREAT | O_EXCL | O_RDWR, S_IRWXA);
        assert!(filefd > 0);
        // create a pipe
        let mut pipefds = PipeArray {
            readfd: -1,
            writefd: -1,
        };
        assert_eq!(cage.pipe2_syscall(&mut pipefds, O_NONBLOCK), 0);
        // contruct a PollStruct with three PollStruct:
        // 1. normal file with non-zero revents, test for revents when the fd is ready
        // 2. negative fd with non-zero revents, even this fd should be ignored, its
        //    revents should still be cleared
        // 3. pipe readfd, test for revents when the fd is not ready
        let mut polled = vec![
            interface::PollStruct {
                fd: filefd,
                events: POLLIN,
                revents: 123,
            },
            interface::PollStruct {
                fd: -1,
                events: POLLIN,
                revents: 123,
            },
            interface::PollStruct {
                fd: pipefds.readfd,
                events: POLLIN,
                revents: 123,
            },
        ];
        // should have exactly one fd ready (file fd)
        assert_eq!(cage.poll_syscall(&mut polled.as_mut_slice(), None), 1);
        assert_eq!(polled[0].revents, POLLIN); // file fd
        assert_eq!(polled[1].revents, 0); // negative fd
        assert_eq!(polled[2].revents, 0); // unready fd

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_net_poll_timeout() {
        // this test is used for testing poll with timeout behaviors specifically

        // acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        // subtest 1: poll when timeout could expire
        // create a TCP AF_INET socket
        let serversockfd = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        let clientsockfd = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        assert!(serversockfd > 0);
        assert!(clientsockfd > 0);

        let port: u16 = generate_random_port();
        let sockaddr = interface::SockaddrV4 {
            sin_family: AF_INET as u16,
            sin_port: port.to_be(),
            sin_addr: interface::V4Addr {
                s_addr: u32::from_ne_bytes([127, 0, 0, 1]),
            },
            padding: 0,
        };
        let socket = interface::GenSockaddr::V4(sockaddr); //127.0.0.1 from bytes above

        // server bind and listen
        assert_eq!(cage.bind_syscall(serversockfd, &socket), 0);
        assert_eq!(cage.listen_syscall(serversockfd, 4), 0);

        assert_eq!(cage.fork_syscall(2), 0);
        assert_eq!(cage.close_syscall(clientsockfd), 0);

        // this barrier is used for preventing
        // an unfixed bug (`close` could block when other thread/cage is `accept`) from
        // deadlocking the test
        let barrier = Arc::new(Barrier::new(2));
        let barrier_2 = barrier.clone();

        //client connects to the server to send and recv data...
        let threadclient = interface::helper_thread(move || {
            let cage2 = interface::cagetable_getref(2);
            assert_eq!(cage2.close_syscall(serversockfd), 0);

            barrier_2.wait();

            // connect to the server
            assert_eq!(cage2.connect_syscall(clientsockfd, &socket), 0);

            // wait for 100ms
            interface::sleep(interface::RustDuration::from_millis(100));

            // send some message to client
            assert_eq!(cage2.send_syscall(clientsockfd, str2cbuf("test"), 4, 0), 4);

            assert_eq!(cage2.close_syscall(clientsockfd), 0);
            cage2.exit_syscall(EXIT_SUCCESS);
        });

        // make sure client thread closed the duplicated socket before server start to
        // accept
        barrier.wait();

        // wait for client to connect
        let mut sockgarbage = interface::GenSockaddr::V4(interface::SockaddrV4::default());
        let sockfd = cage.accept_syscall(serversockfd as i32, &mut sockgarbage);

        // create PollStruct
        let mut polled = vec![interface::PollStruct {
            fd: sockfd,
            events: POLLIN,
            revents: 0,
        }];

        // this counter is used for recording how many times do poll returns due to
        // timeout
        let mut counter = 0;

        loop {
            let poll_result = cage.poll_syscall(
                &mut polled.as_mut_slice(),
                Some(interface::RustDuration::new(0, 10000000)), // 10ms
            );
            assert!(poll_result >= 0);
            // poll timeout after 10ms, but client will send messages after 100ms
            // so there should be some timeout return
            if poll_result == 0 {
                counter += 1;
            } else if polled[0].revents & POLLIN != 0 {
                // just received the message, check the message and break
                let mut buf = sizecbuf(4);
                assert_eq!(cage.recv_syscall(sockfd, buf.as_mut_ptr(), 4, 0), 4);
                assert_eq!(cbuf2str(&buf), "test");
                break;
            } else {
                unreachable!();
            }
        }
        // check if poll timeout correctly
        assert!(counter > 0);

        threadclient.join().unwrap();

        // subtest 2: poll when all arguments were None except for timeout
        // since no set is passed into `poll`, `poll` here should behave like
        // `sleep`
        let start_time = interface::starttimer();
        let timeout = interface::RustDuration::new(0, 10000000); // 10ms
        let poll_result = cage.poll_syscall(&mut vec![].as_mut_slice(), Some(timeout));
        assert!(poll_result == 0);
        // should wait for at least 10ms
        assert!(interface::readtimer(start_time) >= timeout);

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    #[ignore]
    pub fn ut_lind_net_poll() {
        // test for poll monitoring on multiple different file descriptors:
        // 1. regular file
        // 2. AF_INET server socket waiting for two clients
        // 3. AF_INET server socket's connection file descriptor with clients
        // 4. AF_UNIX server socket's connection file descriptor with a client
        // 5. pipe

        // acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();
        let cage = interface::cagetable_getref(1);

        // creating regular file's file descriptor
        let filefd = cage.open_syscall("/netpolltest.txt", O_CREAT | O_EXCL | O_RDWR, S_IRWXA);
        assert!(filefd > 0);

        // creating socket file descriptors
        let serversockfd = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        let clientsockfd1 = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        let clientsockfd2 = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        let serversockfd_unix = cage.socket_syscall(AF_UNIX, SOCK_STREAM, 0);
        let clientsockfd_unix = cage.socket_syscall(AF_UNIX, SOCK_STREAM, 0);

        assert!(serversockfd > 0);
        assert!(clientsockfd1 > 0);
        assert!(clientsockfd2 > 0);
        assert!(serversockfd_unix > 0);
        assert!(clientsockfd_unix > 0);

        // creating a pipe
        let mut pipefds = PipeArray {
            readfd: -1,
            writefd: -1,
        };
        assert_eq!(cage.pipe_syscall(&mut pipefds), 0);

        // create a INET address
        let port: u16 = generate_random_port();

        let sockaddr = interface::SockaddrV4 {
            sin_family: AF_INET as u16,
            sin_port: port.to_be(),
            sin_addr: interface::V4Addr {
                s_addr: u32::from_ne_bytes([127, 0, 0, 1]),
            },
            padding: 0,
        };
        let socket = interface::GenSockaddr::V4(sockaddr); //127.0.0.1 from bytes above

        //binding to a socket
        let serversockaddr_unix =
            interface::new_sockaddr_unix(AF_UNIX as u16, "server_poll".as_bytes());
        let serversocket_unix = interface::GenSockaddr::Unix(serversockaddr_unix);

        let clientsockaddr_unix =
            interface::new_sockaddr_unix(AF_UNIX as u16, "client_poll".as_bytes());
        let clientsocket_unix = interface::GenSockaddr::Unix(clientsockaddr_unix);

        assert_eq!(cage.bind_syscall(serversockfd_unix, &serversocket_unix), 0);
        assert_eq!(cage.bind_syscall(clientsockfd_unix, &clientsocket_unix), 0);
        assert_eq!(cage.listen_syscall(serversockfd_unix, 1), 0);

        assert_eq!(cage.bind_syscall(serversockfd, &socket), 0);
        assert_eq!(cage.listen_syscall(serversockfd, 4), 0);

        // create a PollStruct for each fd
        let serverpoll = interface::PollStruct {
            fd: serversockfd,
            events: POLLIN,
            revents: 0,
        };

        let serverunixpoll = interface::PollStruct {
            fd: serversockfd_unix,
            events: POLLIN,
            revents: 0,
        };

        let filepoll = interface::PollStruct {
            fd: filefd,
            events: POLLIN | POLLOUT,
            revents: 0,
        };

        let pipepoll = interface::PollStruct {
            fd: pipefds.readfd,
            events: POLLIN,
            revents: 0,
        };

        let mut polled = vec![filepoll, serverpoll, serverunixpoll, pipepoll];

        assert_eq!(cage.fork_syscall(2), 0); // used for AF_INET thread client 1
        assert_eq!(cage.fork_syscall(3), 0); // used for AF_INET thread client 2
        assert_eq!(cage.fork_syscall(4), 0); // used for AF_UNIX thread client

        assert_eq!(cage.fork_syscall(5), 0); // used for pipe thread

        assert_eq!(cage.close_syscall(clientsockfd1), 0);
        assert_eq!(cage.close_syscall(clientsockfd2), 0);
        assert_eq!(cage.close_syscall(clientsockfd_unix), 0);

        // this barrier have to ensure that the clients finish the connect before we do
        // the poll due to an unfixed bug (`close` could block when other
        // thread/cage is `accept`)
        let barrier = Arc::new(Barrier::new(3));
        let barrier_clone1 = barrier.clone();
        let barrier_clone2 = barrier.clone();

        // this barrier is used for control the flow the pipe
        let barrier_pipe = Arc::new(Barrier::new(2));
        let barrier_pipe_clone = barrier_pipe.clone();

        // due to an unfixed bug in ref counter of AF_UNIX socket pipe
        // have to make sure all the threads exits only after the AF_UNIX test finished
        let barrier_exit = Arc::new(Barrier::new(4));
        let barrier_exit_clone1 = barrier_exit.clone();
        let barrier_exit_clone2 = barrier_exit.clone();
        let barrier_exit_clone3 = barrier_exit.clone();

        // client 1 connects to the server to send and recv data
        let threadclient1 = interface::helper_thread(move || {
            let cage2 = interface::cagetable_getref(2);
            assert_eq!(cage2.close_syscall(serversockfd), 0);
            assert_eq!(cage2.close_syscall(clientsockfd2), 0);

            // connect to server
            assert_eq!(cage2.connect_syscall(clientsockfd1, &socket), 0);
            barrier_clone1.wait();

            // send message to server
            assert_eq!(cage2.send_syscall(clientsockfd1, str2cbuf("test"), 4, 0), 4);

            interface::sleep(interface::RustDuration::from_millis(1));

            // receive message from server
            let mut buf = sizecbuf(4);
            assert_eq!(cage2.recv_syscall(clientsockfd1, buf.as_mut_ptr(), 4, 0), 4);
            assert_eq!(cbuf2str(&buf), "test");

            assert_eq!(cage2.close_syscall(clientsockfd1), 0);
            barrier_exit_clone1.wait();
            cage2.exit_syscall(EXIT_SUCCESS);
        });

        // client 2 connects to the server to send and recv data
        let threadclient2 = interface::helper_thread(move || {
            let cage3 = interface::cagetable_getref(3);
            assert_eq!(cage3.close_syscall(serversockfd), 0);
            assert_eq!(cage3.close_syscall(clientsockfd1), 0);

            // connect to server
            assert_eq!(cage3.connect_syscall(clientsockfd2, &socket), 0);
            barrier_clone2.wait();

            // send message to server
            assert_eq!(cage3.send_syscall(clientsockfd2, str2cbuf("test"), 4, 0), 4);

            interface::sleep(interface::RustDuration::from_millis(1));

            // receive message from server
            let mut buf = sizecbuf(4);
            let mut result: i32;
            loop {
                result = cage3.recv_syscall(clientsockfd2, buf.as_mut_ptr(), 4, 0);
                if result != -libc::EINTR {
                    break; // if the error was EINTR, retry the syscall
                }
            }
            assert_eq!(result, 4);
            assert_eq!(cbuf2str(&buf), "test");

            assert_eq!(cage3.close_syscall(clientsockfd2), 0);
            barrier_exit_clone2.wait();
            cage3.exit_syscall(EXIT_SUCCESS);
        });

        let threadclient_unix = interface::helper_thread(move || {
            let cage4 = interface::cagetable_getref(4);
            assert_eq!(cage4.close_syscall(serversockfd_unix), 0);
            assert_eq!(cage4.close_syscall(serversockfd), 0);

            // connect to server
            assert_eq!(
                cage4.connect_syscall(clientsockfd_unix, &serversocket_unix),
                0
            );

            // send message to server
            assert_eq!(
                cage4.send_syscall(clientsockfd_unix, str2cbuf("test"), 4, 0),
                4
            );

            interface::sleep(interface::RustDuration::from_millis(1));

            // recieve message from server
            let mut buf = sizecbuf(4);
            let mut result: i32;
            loop {
                result = cage4.recv_syscall(clientsockfd_unix, buf.as_mut_ptr(), 4, 0);
                if result != -libc::EINTR {
                    break; // if the error was EINTR, retry the syscall
                }
            }
            assert_eq!(result, 4);
            assert_eq!(cbuf2str(&buf), "test");

            assert_eq!(cage4.close_syscall(clientsockfd_unix), 0);
            cage4.exit_syscall(EXIT_SUCCESS);
        });

        let thread_pipe = interface::helper_thread(move || {
            let cage5 = interface::cagetable_getref(5);

            interface::sleep(interface::RustDuration::from_millis(1));
            // send message to pipe
            assert_eq!(cage5.write_syscall(pipefds.writefd, str2cbuf("test"), 4), 4);

            let mut buf = sizecbuf(5);
            // wait until peer read the message
            barrier_pipe_clone.wait();

            // read the message sent by peer
            assert_eq!(cage5.read_syscall(pipefds.readfd, buf.as_mut_ptr(), 5), 5);
            assert_eq!(cbuf2str(&buf), "test2");

            barrier_exit_clone3.wait();
            cage5.exit_syscall(EXIT_SUCCESS);
        });

        barrier.wait();
        // acting as the server and processing the request
        // Server loop to handle connections and I/O
        // Check for any activity in any of the Input sockets
        for counter in 0..600 {
            let poll_result = cage.poll_syscall(&mut polled.as_mut_slice(), None);
            assert!(poll_result >= 0); // check for error

            // clearfds stores the fds that should be removed from polled at the end of the
            // iteration
            let mut clearfds = vec![];
            // addfds stores the fds that should be added to polled at the end of the
            // iteration
            let mut addfds = vec![];

            // check for readfds
            for poll in &mut polled {
                // If the socket returned was listerner socket, then there's a new conn., so we
                // accept it, and put the client socket in the list of Inputs.
                if poll.fd == serversockfd {
                    if poll.revents & POLLIN != 0 {
                        let mut sockgarbage =
                            interface::GenSockaddr::V4(interface::SockaddrV4::default());
                        let sockfd = cage.accept_syscall(poll.fd as i32, &mut sockgarbage);
                        assert!(sockfd > 0);
                        // new connection is estalished, add it to readfds and writefds

                        addfds.push(interface::PollStruct {
                            fd: sockfd,
                            events: POLLIN | POLLOUT,
                            revents: 0,
                        });
                    }
                } else if poll.fd == filefd {
                    // poll on regular file should always success
                    // therefore revents should be set for filefd at the first iteration
                    assert_eq!(counter, 0);
                    assert_eq!(poll.revents, POLLIN | POLLOUT);
                    // remove file fd from poll
                    clearfds.push(filefd);
                } else if poll.fd == serversockfd_unix {
                    if poll.revents & POLLIN != 0 {
                        // unix socket
                        let mut sockgarbage = interface::GenSockaddr::Unix(
                            interface::new_sockaddr_unix(AF_UNIX as u16, "".as_bytes()),
                        );
                        let sockfd = cage.accept_syscall(poll.fd as i32, &mut sockgarbage);
                        assert!(sockfd > 0);
                        // new connection is estalished, add it to poll
                        addfds.push(interface::PollStruct {
                            fd: sockfd,
                            events: POLLIN | POLLOUT,
                            revents: 0,
                        });
                    }
                } else if poll.fd == pipefds.readfd {
                    if poll.revents & POLLIN != 0 {
                        // pipe
                        let mut buf = sizecbuf(4);
                        // read the message from peer
                        assert_eq!(cage.read_syscall(pipefds.readfd, buf.as_mut_ptr(), 4), 4);
                        assert_eq!(cbuf2str(&buf), "test");

                        // write the message from peer
                        assert_eq!(
                            cage.write_syscall(pipefds.writefd, str2cbuf("test2"), 5) as usize,
                            5
                        );
                        barrier_pipe.wait();

                        // pipe poll test done
                        clearfds.push(pipefds.readfd);
                    }
                } else {
                    if poll.revents & POLLIN != 0 {
                        //If the socket is in established conn., then we recv the data. If there's
                        // no data, then close the client socket.
                        let mut buf = sizecbuf(4);
                        let mut recvresult: i32;
                        loop {
                            // receive message from peer
                            recvresult = cage.recv_syscall(poll.fd as i32, buf.as_mut_ptr(), 4, 0);
                            if recvresult != -libc::EINTR {
                                break; // if the error was EINTR, retry the
                                       // syscall
                            }
                        }
                        if recvresult == 4 {
                            if cbuf2str(&buf) == "test" {
                                continue;
                            }
                        } else if recvresult == -libc::ECONNRESET {
                            // peer closed the connection
                            println!("Connection reset by peer on socket {}", poll.fd);
                            assert_eq!(cage.close_syscall(poll.fd as i32), 0);
                            clearfds.push(poll.fd);
                        }
                    }
                    if poll.revents & POLLOUT != 0 {
                        // Data is sent out this socket, it's no longer ready for writing
                        // clear the POLLOUT from events
                        assert_eq!(cage.send_syscall(poll.fd as i32, str2cbuf("test"), 4, 0), 4);
                        poll.events &= !POLLOUT;
                    }
                }
            }
            // clear fds
            polled.retain(|x| {
                for fd in &clearfds {
                    if *fd == x.fd {
                        return false;
                    }
                }
                return true;
            });
            // add new fds
            polled.extend(addfds);
        }
        assert_eq!(cage.close_syscall(serversockfd), 0);
        assert_eq!(cage.close_syscall(serversockfd_unix), 0);

        // let threads exit
        barrier_exit.wait();

        threadclient1.join().unwrap();
        threadclient2.join().unwrap();
        threadclient_unix.join().unwrap();
        thread_pipe.join().unwrap();

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_net_recvfrom() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        let serversockfd = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        let clientsockfd = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);

        //making sure that the assigned fd's are valid
        assert!(serversockfd > 0);
        assert!(clientsockfd > 0);
        let port: u16 = generate_random_port();
        //binding to a socket
        let sockaddr = interface::SockaddrV4 {
            sin_family: AF_INET as u16,
            sin_port: port.to_be(),
            sin_addr: interface::V4Addr {
                s_addr: u32::from_ne_bytes([127, 0, 0, 1]),
            },
            padding: 0,
        };
        let socket = interface::GenSockaddr::V4(sockaddr); //127.0.0.1
        assert_eq!(cage.bind_syscall(serversockfd, &socket), 0);
        assert_eq!(cage.listen_syscall(serversockfd, 1), 0); //we are only allowing for one client at a time

        //forking the cage to get another cage with the same information
        assert_eq!(cage.fork_syscall(2), 0);

        //creating a thread for the server so that the information can be sent between
        // the two threads
        let thread = interface::helper_thread(move || {
            let cage2 = interface::cagetable_getref(2);
            interface::sleep(interface::RustDuration::from_millis(100));
            let port: u16 = generate_random_port();

            let mut socket2 = interface::GenSockaddr::V4(interface::SockaddrV4 {
                sin_family: AF_INET as u16,
                sin_port: port.to_be(),
                sin_addr: interface::V4Addr {
                    s_addr: u32::from_ne_bytes([127, 0, 0, 1]),
                },
                padding: 0,
            }); //127.0.0.1
            let sockfd = cage2.accept_syscall(serversockfd, &mut socket2); //really can only make sure that the fd is valid
            assert!(sockfd > 0);

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
            buf = sizecbuf(100);
            assert_eq!(
                cage2.recvfrom_syscall(sockfd, buf.as_mut_ptr(), 100, 0, &mut Some(&mut socket2)),
                100
            ); //reading the input message
            buf = sizecbuf(100);

            interface::sleep(interface::RustDuration::from_millis(200));

            //process the second test...
            //Writing 100, read 20, peek 20, read 80
            assert_eq!(
                cage2.recvfrom_syscall(sockfd, buf.as_mut_ptr(), 20, 0, &mut Some(&mut socket2)),
                20
            );
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
            buf = sizecbuf(100);
            assert_eq!(
                cage2.recvfrom_syscall(sockfd, buf.as_mut_ptr(), 80, 0, &mut Some(&mut socket2)),
                80
            );
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
                buf = sizecbuf(100);
            }
            assert_eq!(
                cage2.recvfrom_syscall(sockfd, buf.as_mut_ptr(), 100, 0, &mut Some(&mut socket2)),
                100
            );
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

            interface::sleep(interface::RustDuration::from_millis(100));

            assert_eq!(cage2.close_syscall(sockfd), 0);
            assert_eq!(cage2.close_syscall(serversockfd), 0);
            assert_eq!(cage2.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        });

        //connect to the server
        assert_eq!(cage.connect_syscall(clientsockfd, &socket), 0);

        //send the data with delays so that the server can process the information
        // cleanly
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

        thread.join().unwrap();

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_net_select_badinput() {
        // this test is used for testing select with error cases

        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();
        let cage = interface::cagetable_getref(1);

        let sets = &mut interface::FdSet::new();

        // test for invalid file descriptor error
        // 5 is an invalid file descriptor
        sets.set(5 as i32);
        assert_eq!(
            cage.select_syscall(6, Some(sets), None, None, None,),
            -(Errno::EBADF as i32)
        );

        // test for invalid file descriptor range
        // negative number
        assert_eq!(
            cage.select_syscall(-1, None, None, None, None,),
            -(Errno::EINVAL as i32)
        );

        //
        assert_eq!(
            cage.select_syscall(FD_SET_MAX_FD + 1, None, None, None, None,),
            -(Errno::EINVAL as i32)
        );

        // test for signal while in select
        // TO-DO: sending signals using kill_syscall is
        // currently not supported in raw safeposix environment

        // let thread = interface::helper_thread(move || {
        //     let cage = interface::cagetable_getref(1);
        //     cage.kill_syscall(1, SIGUSR1);
        // });

        // assert_eq!(cage.select_syscall(
        //     0,
        //     None,
        //     None,
        //     None,
        //     None,
        // ), -(Errno::EINTR as i32));

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_net_select_timeout() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();
        let cage = interface::cagetable_getref(1);

        // subtest 1: select when timeout could expire
        // create a TCP AF_UNIX socket
        let serversockfd = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        let clientsockfd = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        assert!(serversockfd > 0);
        assert!(clientsockfd > 0);

        let port: u16 = generate_random_port();
        let sockaddr = interface::SockaddrV4 {
            sin_family: AF_INET as u16,
            sin_port: port.to_be(),
            sin_addr: interface::V4Addr {
                s_addr: u32::from_ne_bytes([127, 0, 0, 1]),
            },
            padding: 0,
        };
        let socket = interface::GenSockaddr::V4(sockaddr); //127.0.0.1 from bytes above

        // server bind and listen
        assert_eq!(cage.bind_syscall(serversockfd, &socket), 0);
        assert_eq!(cage.listen_syscall(serversockfd, 4), 0);

        assert_eq!(cage.fork_syscall(2), 0);
        assert_eq!(cage.close_syscall(clientsockfd), 0);

        // this barrier is used for preventing
        // an unfixed bug (`close` could block when other thread/cage is `accept`) from
        // deadlocking the test
        let barrier = Arc::new(Barrier::new(2));
        let barrier_2 = barrier.clone();

        //client connects to the server to send and recv data...
        let threadclient = interface::helper_thread(move || {
            let cage2 = interface::cagetable_getref(2);
            assert_eq!(cage2.close_syscall(serversockfd), 0);

            barrier_2.wait();

            // connect to the server
            assert_eq!(cage2.connect_syscall(clientsockfd, &socket), 0);

            // wait for 100ms
            interface::sleep(interface::RustDuration::from_millis(100));

            // send some message to client
            assert_eq!(cage2.send_syscall(clientsockfd, str2cbuf("test"), 4, 0), 4);

            assert_eq!(cage2.close_syscall(clientsockfd), 0);
            cage2.exit_syscall(EXIT_SUCCESS);
        });

        // make sure client thread closed the duplicated socket before server start to
        // accept
        barrier.wait();

        // wait for client to connect
        let mut sockgarbage = interface::GenSockaddr::V4(interface::SockaddrV4::default());
        let sockfd = cage.accept_syscall(serversockfd as i32, &mut sockgarbage);

        // add to client socket to fdset
        let master_sets = &mut interface::FdSet::new();
        master_sets.set(sockfd);

        // this counter is used for recording how many times do select returns due to
        // timeout
        let mut counter = 0;

        loop {
            let sets = &mut interface::FdSet::new();
            sets.copy_from(master_sets);
            let select_result = cage.select_syscall(
                sockfd + 1,
                Some(sets),
                None,
                None,
                Some(interface::RustDuration::new(0, 10000000)), // 10ms
            );
            assert!(select_result >= 0);
            // select timeout after 10ms, but client will send messages after 100ms
            // so there should be some timeout return
            if select_result == 0 {
                counter += 1;
            } else if sets.is_set(sockfd) {
                // just received the message, check the message and break
                let mut buf = sizecbuf(4);
                assert_eq!(cage.recv_syscall(sockfd, buf.as_mut_ptr(), 4, 0), 4);
                assert_eq!(cbuf2str(&buf), "test");
                break;
            } else {
                unreachable!();
            }
        }
        // check if select timeout correctly
        assert!(counter > 0);

        threadclient.join().unwrap();

        // subtest 2: select when all arguments were None except for timeout
        // since no set is passed into `select`, `select` here should behave like
        // `sleep`
        let start_time = interface::starttimer();
        let timeout = interface::RustDuration::new(0, 10000000); // 10ms
        let select_result = cage.select_syscall(sockfd + 1, None, None, None, Some(timeout));
        assert!(select_result == 0);
        // should wait for at least 10ms
        assert!(interface::readtimer(start_time) >= timeout);

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_net_select_pipe_write_blocking() {
        // this test is used for testing select on pipe writefds
        // PIPE_CAPACITY: the maximum size of a pipe buffer
        let byte_chunk: usize = PIPE_CAPACITY;

        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();
        let cage = interface::cagetable_getref(1);

        // create a pipe
        let mut pipefds = PipeArray {
            readfd: -1,
            writefd: -1,
        };

        // we use nonblocking mode since we can check if pipe is ready to read/write
        // easily by checking if the read/write is returning EAGAIN
        assert_eq!(cage.pipe2_syscall(&mut pipefds, O_NONBLOCK), 0);
        assert_eq!(cage.fork_syscall(2), 0);

        // this barrier is for better control about when receiver should consume the
        // data
        let barrier = Arc::new(Barrier::new(2));
        let barrier_clone = barrier.clone();

        let receiver = interface::helper_thread(move || {
            let cage2 = interface::cagetable_getref(2);

            // receiver end: close writefd, dup readfd
            assert_eq!(cage2.close_syscall(pipefds.writefd), 0);
            assert_eq!(cage2.dup2_syscall(pipefds.readfd, 0), 0);
            assert_eq!(cage2.close_syscall(pipefds.readfd), 0);

            // used for holding read_syscall return
            let mut bytes_read: i32 = 1;

            // receiver buffer
            let mut buf: Vec<u8> = Vec::with_capacity(byte_chunk);
            let bufptr = buf.as_mut_ptr();

            // make a barrier before receiver started to consume data
            barrier_clone.wait();

            // before actually started to consume data, sleep for 10ms
            // to test if select is really going to wait
            interface::sleep(interface::RustDuration::from_millis(10));

            while bytes_read != 0 {
                // consume the data until peer closed the pipe
                bytes_read = cage2.read_syscall(0, bufptr, byte_chunk);
                if bytes_read == -(Errno::EAGAIN as i32) {
                    continue;
                }
                assert!(bytes_read <= byte_chunk as i32);
            }

            assert_eq!(cage2.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        });

        // sender end: close readfd, dup writefd
        assert_eq!(cage.close_syscall(pipefds.readfd), 0);
        assert_eq!(cage.dup2_syscall(pipefds.writefd, 1), 1);
        assert_eq!(cage.close_syscall(pipefds.writefd), 0);

        // first write will fill up the entire pipe
        let mut buf: Vec<u8> = vec!['A' as u8; byte_chunk];
        let bufptr = buf.as_mut_ptr();
        assert_eq!(
            cage.write_syscall(1, bufptr, byte_chunk) as usize,
            byte_chunk
        );

        // since pipe is already filled up, following write should fail
        assert_eq!(
            cage.write_syscall(1, bufptr, byte_chunk),
            -(Errno::EAGAIN as i32)
        );

        let outputs = &mut interface::FdSet::new();
        outputs.set(1);

        // release the barrier and let receiver consume the data
        barrier.wait();

        let select_result = cage.select_syscall(2, None, Some(outputs), None, None);
        assert!(select_result == 1); // should have exactly one file descriptor ready

        // all the data are just consumed by the receiver, now the write should be
        // successful
        assert_ne!(
            cage.write_syscall(1, bufptr, byte_chunk),
            -(Errno::EAGAIN as i32)
        );

        // close the pipe
        assert_eq!(cage.close_syscall(1), 0);

        receiver.join().unwrap();

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    #[ignore]
    pub fn ut_lind_net_select_socket_write_blocking() {
        // this test is used for testing select on AF_UNIX socket pipe writefds
        // currently would fail since select_syscall does not handle socket pipe
        // writefds correctly
        let byte_chunk: usize = UDSOCK_CAPACITY;

        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();
        let cage = interface::cagetable_getref(1);

        // create a AF_UNIX socket
        // we use nonblocking mode since we can check if pipe is ready to read/write
        // easily by checking if the read/write is returning EAGAIN
        let serversockfd_unix = cage.socket_syscall(AF_UNIX, SOCK_STREAM | SOCK_NONBLOCK, 0);
        let clientsockfd_unix = cage.socket_syscall(AF_UNIX, SOCK_STREAM | SOCK_NONBLOCK, 0);

        //binding to a socket
        let serversockaddr_unix =
            interface::new_sockaddr_unix(AF_UNIX as u16, "server_select".as_bytes());
        let serversocket_unix = interface::GenSockaddr::Unix(serversockaddr_unix);

        let clientsockaddr_unix =
            interface::new_sockaddr_unix(AF_UNIX as u16, "client_select".as_bytes());
        let clientsocket_unix = interface::GenSockaddr::Unix(clientsockaddr_unix);

        assert_eq!(cage.bind_syscall(serversockfd_unix, &serversocket_unix), 0);
        assert_eq!(cage.bind_syscall(clientsockfd_unix, &clientsocket_unix), 0);
        assert_eq!(cage.listen_syscall(serversockfd_unix, 1), 0);

        assert_eq!(cage.fork_syscall(2), 0);

        // this barrier is for better control about when receiver should consume the
        // data
        let barrier = Arc::new(Barrier::new(2));
        let barrier_clone = barrier.clone();

        let threadclient_unix = interface::helper_thread(move || {
            let cage2 = interface::cagetable_getref(2);
            assert_eq!(cage2.close_syscall(serversockfd_unix), 0);

            // connect to the server
            assert_eq!(
                cage2.connect_syscall(clientsockfd_unix, &serversocket_unix),
                0
            );

            // receiver buffer
            let mut buf: Vec<u8> = Vec::with_capacity(byte_chunk);
            let bufptr = buf.as_mut_ptr();
            let mut result: i32 = 1;

            // make a barrier before receiver started to consume data
            barrier_clone.wait();

            // before actually started to consume data, sleep for 10ms
            // to test if select is really going to wait
            interface::sleep(interface::RustDuration::from_millis(10));

            while result != 0 {
                // consume the data until peer closed the socket
                result = cage2.recv_syscall(clientsockfd_unix, bufptr, byte_chunk, 0);
                if result == -(Errno::EAGAIN as i32) {
                    continue;
                }
                assert!(result <= byte_chunk as i32);
            }

            assert_eq!(cage2.close_syscall(clientsockfd_unix), 0);
            cage2.exit_syscall(EXIT_SUCCESS);
        });

        // this sleep is to prevent an unfixed bug (`close` would block when other
        // thread/cage is `accept`) from deadlocking the test
        interface::sleep(interface::RustDuration::from_millis(10));

        let mut sockgarbage = interface::GenSockaddr::V4(interface::SockaddrV4::default());
        let sockfd = cage.accept_syscall(serversockfd_unix as i32, &mut sockgarbage);

        // sender buffer
        let mut buf: Vec<u8> = vec!['A' as u8; byte_chunk];
        let bufptr = buf.as_mut_ptr();

        // first send will fill up the entire socket pipe
        assert_eq!(
            cage.send_syscall(sockfd, bufptr, byte_chunk, 0) as usize,
            byte_chunk
        );
        // since socket pipe is already filled up, following send should fail
        assert_eq!(
            cage.send_syscall(sockfd, bufptr, byte_chunk, 0),
            -(Errno::EAGAIN as i32)
        );

        let outputs = &mut interface::FdSet::new();
        outputs.set(sockfd);

        // release the barrier and let receiver consume the data
        barrier.wait();

        let select_result = cage.select_syscall(sockfd + 1, None, Some(outputs), None, None);
        assert!(select_result == 1); // should have exactly one file descriptor ready

        // all the data are just consumed by the receiver, now the write should be
        // successful
        assert_ne!(
            cage.send_syscall(sockfd, bufptr, byte_chunk, 0),
            -(Errno::EAGAIN as i32)
        );

        assert_eq!(cage.close_syscall(sockfd), 0);

        threadclient_unix.join().unwrap();

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    #[ignore]
    pub fn ut_lind_net_select() {
        // test for select monitoring on multiple different file descriptors:
        // 1. regular file
        // 2. AF_INET server socket waiting for two clients
        // 3. AF_INET server socket's connection file descriptor with clients
        // 4. AF_UNIX server socket's connection file descriptor with a client
        // 5. pipe

        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();
        let cage = interface::cagetable_getref(1);

        // creating regular file's file descriptor
        let filefd = cage.open_syscall("/netselecttest.txt", O_CREAT | O_EXCL | O_RDWR, S_IRWXA);
        assert!(filefd > 0);

        // creating socket file descriptors
        let serversockfd = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        let clientsockfd1 = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        let clientsockfd2 = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        let serversockfd_unix = cage.socket_syscall(AF_UNIX, SOCK_STREAM, 0);
        let clientsockfd_unix = cage.socket_syscall(AF_UNIX, SOCK_STREAM, 0);

        // record the maximum fd number to pass as nfds of select
        let mut max_fds = 2;

        assert!(serversockfd > 0);
        assert!(clientsockfd1 > 0);
        assert!(clientsockfd2 > 0);
        assert!(serversockfd_unix > 0);
        assert!(clientsockfd_unix > 0);

        // creating a pipe
        let mut pipefds = PipeArray {
            readfd: -1,
            writefd: -1,
        };
        assert_eq!(cage.pipe_syscall(&mut pipefds), 0);

        // create a INET address
        let port: u16 = generate_random_port();

        let sockaddr = interface::SockaddrV4 {
            sin_family: AF_INET as u16,
            sin_port: port.to_be(),
            sin_addr: interface::V4Addr {
                s_addr: u32::from_ne_bytes([127, 0, 0, 1]),
            },
            padding: 0,
        };
        let socket = interface::GenSockaddr::V4(sockaddr); //127.0.0.1 from bytes above

        //binding to a socket
        let serversockaddr_unix =
            interface::new_sockaddr_unix(AF_UNIX as u16, "server_select".as_bytes());
        let serversocket_unix = interface::GenSockaddr::Unix(serversockaddr_unix);

        let clientsockaddr_unix =
            interface::new_sockaddr_unix(AF_UNIX as u16, "client_select".as_bytes());
        let clientsocket_unix = interface::GenSockaddr::Unix(clientsockaddr_unix);

        assert_eq!(cage.bind_syscall(serversockfd_unix, &serversocket_unix), 0);
        assert_eq!(cage.bind_syscall(clientsockfd_unix, &clientsocket_unix), 0);
        assert_eq!(cage.listen_syscall(serversockfd_unix, 1), 0);

        assert_eq!(cage.bind_syscall(serversockfd, &socket), 0);
        assert_eq!(cage.listen_syscall(serversockfd, 4), 0);

        // allocate spaces for fd_set bitmaps
        // `master_set`: Consits of all read file descriptors.
        // `working_set`: Consits of a copy of `master_set`. Modified by `select()` to
        // contain only ready descriptors. `master_outputs_set`: Consits of all
        // write file descriptors. `outputs`: Consits of a copy of
        // `master_outputs_set`. Modified by `select()` to contain only ready
        // descriptors.
        let master_set = &mut interface::FdSet::new();
        let working_set = &mut interface::FdSet::new();
        let master_outputs_set = &mut interface::FdSet::new();
        let outputs = &mut interface::FdSet::new();

        // readfds
        master_set.set(serversockfd); // AF_INET socket server fd
        master_set.set(serversockfd_unix); // AF_UNIX socket server fd
        master_set.set(filefd); // regular file fd
        master_set.set(pipefds.readfd); // pipe fd

        // writefds
        master_outputs_set.set(filefd); // regular file fd

        // update max_fds
        max_fds = std::cmp::max(max_fds, serversockfd);
        max_fds = std::cmp::max(max_fds, serversockfd_unix);
        max_fds = std::cmp::max(max_fds, filefd);
        max_fds = std::cmp::max(max_fds, pipefds.readfd);

        // check if FdSet works correctly
        assert_eq!(master_set.is_set(serversockfd), true);
        assert_eq!(master_set.is_set(serversockfd_unix), true);
        assert_eq!(master_set.is_set(filefd), true);
        assert_eq!(master_set.is_set(pipefds.readfd), true);
        assert_eq!(master_outputs_set.is_set(filefd), true);

        assert_eq!(cage.fork_syscall(2), 0); // used for AF_INET thread client 1
        assert_eq!(cage.fork_syscall(3), 0); // used for AF_INET thread client 2
        assert_eq!(cage.fork_syscall(4), 0); // used for AF_UNIX thread client

        assert_eq!(cage.fork_syscall(5), 0); // used for pipe thread

        assert_eq!(cage.close_syscall(clientsockfd1), 0);
        assert_eq!(cage.close_syscall(clientsockfd2), 0);
        assert_eq!(cage.close_syscall(clientsockfd_unix), 0);

        // this barrier have to ensure that the clients finish the connect before we do
        // the select due to an unfixed bug (`close` could block when other
        // thread/cage is `accept`)
        let barrier = Arc::new(Barrier::new(3));
        let barrier_clone1 = barrier.clone();
        let barrier_clone2 = barrier.clone();

        // this barrier is used for control the flow the pipe
        let barrier_pipe = Arc::new(Barrier::new(2));
        let barrier_pipe_clone = barrier_pipe.clone();

        // due to an unfixed bug in ref counter of AF_UNIX socket pipe
        // have to make sure all the threads exits only after the AF_UNIX test finished
        let barrier_exit = Arc::new(Barrier::new(4));
        let barrier_exit_clone1 = barrier_exit.clone();
        let barrier_exit_clone2 = barrier_exit.clone();
        let barrier_exit_clone3 = barrier_exit.clone();

        //client 1 connects to the server to send and recv data...
        let threadclient1 = interface::helper_thread(move || {
            let cage2 = interface::cagetable_getref(2);
            assert_eq!(cage2.close_syscall(serversockfd), 0);
            assert_eq!(cage2.close_syscall(clientsockfd2), 0);

            // connect to server
            assert_eq!(cage2.connect_syscall(clientsockfd1, &socket), 0);
            barrier_clone1.wait();

            // send message to server
            assert_eq!(cage2.send_syscall(clientsockfd1, str2cbuf("test"), 4, 0), 4);

            interface::sleep(interface::RustDuration::from_millis(1));

            // receive message from server
            let mut buf = sizecbuf(4);
            assert_eq!(cage2.recv_syscall(clientsockfd1, buf.as_mut_ptr(), 4, 0), 4);
            assert_eq!(cbuf2str(&buf), "test");

            assert_eq!(cage2.close_syscall(clientsockfd1), 0);
            barrier_exit_clone1.wait();
            cage2.exit_syscall(EXIT_SUCCESS);
        });

        //client 2 connects to the server to send and recv data...
        let threadclient2 = interface::helper_thread(move || {
            let cage3 = interface::cagetable_getref(3);
            assert_eq!(cage3.close_syscall(serversockfd), 0);
            assert_eq!(cage3.close_syscall(clientsockfd1), 0);

            // connect to server
            assert_eq!(cage3.connect_syscall(clientsockfd2, &socket), 0);
            barrier_clone2.wait();

            // send message to server
            assert_eq!(cage3.send_syscall(clientsockfd2, str2cbuf("test"), 4, 0), 4);

            interface::sleep(interface::RustDuration::from_millis(1));

            // receive message from server
            let mut buf = sizecbuf(4);
            let mut result: i32;
            loop {
                result = cage3.recv_syscall(clientsockfd2, buf.as_mut_ptr(), 4, 0);
                if result != -libc::EINTR {
                    break; // if the error was EINTR, retry the syscall
                }
            }
            assert_eq!(result, 4);
            assert_eq!(cbuf2str(&buf), "test");

            assert_eq!(cage3.close_syscall(clientsockfd2), 0);
            barrier_exit_clone2.wait();
            cage3.exit_syscall(EXIT_SUCCESS);
        });

        let threadclient_unix = interface::helper_thread(move || {
            let cage4 = interface::cagetable_getref(4);
            assert_eq!(cage4.close_syscall(serversockfd_unix), 0);
            assert_eq!(cage4.close_syscall(serversockfd), 0);

            // connect to server
            assert_eq!(
                cage4.connect_syscall(clientsockfd_unix, &serversocket_unix),
                0
            );

            // send message to server
            assert_eq!(
                cage4.send_syscall(clientsockfd_unix, str2cbuf("test"), 4, 0),
                4
            );

            interface::sleep(interface::RustDuration::from_millis(1));

            // recieve message from server
            let mut buf = sizecbuf(4);
            let mut result: i32;
            loop {
                result = cage4.recv_syscall(clientsockfd_unix, buf.as_mut_ptr(), 4, 0);
                if result != -libc::EINTR {
                    break; // if the error was EINTR, retry the syscall
                }
            }
            assert_eq!(result, 4);
            assert_eq!(cbuf2str(&buf), "test");

            assert_eq!(cage4.close_syscall(clientsockfd_unix), 0);
            cage4.exit_syscall(EXIT_SUCCESS);
        });

        let thread_pipe = interface::helper_thread(move || {
            let cage5 = interface::cagetable_getref(5);

            interface::sleep(interface::RustDuration::from_millis(1));
            // send message to pipe
            assert_eq!(cage5.write_syscall(pipefds.writefd, str2cbuf("test"), 4), 4);

            let mut buf = sizecbuf(5);
            // wait until peer read the message
            barrier_pipe_clone.wait();

            // read the message sent by peer
            assert_eq!(cage5.read_syscall(pipefds.readfd, buf.as_mut_ptr(), 5), 5);
            assert_eq!(cbuf2str(&buf), "test2");

            barrier_exit_clone3.wait();
            cage5.exit_syscall(EXIT_SUCCESS);
        });

        barrier.wait();
        // acting as the server and processing the request
        // Server loop to handle connections and I/O
        // Check for any activity in any of the Input sockets...
        for _counter in 0..600 {
            working_set.copy_from(master_set);
            outputs.copy_from(master_outputs_set);
            let select_result =
                cage.select_syscall(max_fds + 1, Some(working_set), Some(outputs), None, None);
            assert!(select_result >= 0); // check for error

            // check for readfds
            for sock in 0..=max_fds {
                if !working_set.is_set(sock) {
                    continue;
                }
                //If the socket returned was listerner socket, then there's a new conn., so we
                // accept it, and put the client socket in the list of Inputs.
                if sock == serversockfd {
                    let mut sockgarbage =
                        interface::GenSockaddr::V4(interface::SockaddrV4::default());
                    let sockfd = cage.accept_syscall(sock as i32, &mut sockgarbage);
                    assert!(sockfd > 0);
                    // new connection is estalished, add it to readfds and writefds
                    master_set.set(sockfd);
                    master_outputs_set.set(sockfd);
                    // update max_fds
                    max_fds = std::cmp::max(max_fds, sockfd);
                } else if sock == filefd {
                    //Write to a file...
                    assert_eq!(cage.write_syscall(sock as i32, str2cbuf("test"), 4), 4);
                    assert_eq!(cage.lseek_syscall(sock as i32, 0, SEEK_SET), 0);
                    master_set.clear(sock);
                    // regular file select test done
                } else if sock == serversockfd_unix {
                    // unix socket
                    let mut sockgarbage = interface::GenSockaddr::Unix(
                        interface::new_sockaddr_unix(AF_UNIX as u16, "".as_bytes()),
                    );
                    let sockfd = cage.accept_syscall(sock as i32, &mut sockgarbage);
                    assert!(sockfd > 0);
                    // new connection is estalished, add it to readfds and writefds
                    master_set.set(sockfd);
                    master_outputs_set.set(sockfd);
                    // update max_fds
                    max_fds = std::cmp::max(max_fds, sockfd);
                } else if sock == pipefds.readfd {
                    // pipe
                    let mut buf = sizecbuf(4);
                    // read the message from peer
                    assert_eq!(cage.read_syscall(sock, buf.as_mut_ptr(), 4), 4);
                    assert_eq!(cbuf2str(&buf), "test");

                    // write the message from peer
                    assert_eq!(
                        cage.write_syscall(pipefds.writefd, str2cbuf("test2"), 5) as usize,
                        5
                    );
                    barrier_pipe.wait();

                    // pipe select test done
                    master_set.clear(sock);
                } else {
                    //If the socket is in established conn., then we recv the data. If there's no
                    // data, then close the client socket.
                    let mut buf = sizecbuf(4);
                    let mut recvresult: i32;
                    loop {
                        // receive message from peer
                        recvresult = cage.recv_syscall(sock as i32, buf.as_mut_ptr(), 4, 0);
                        if recvresult != -libc::EINTR {
                            break; // if the error was EINTR, retry the syscall
                        }
                    }
                    if recvresult == 4 {
                        if cbuf2str(&buf) == "test" {
                            continue;
                        }
                    } else if recvresult == -libc::ECONNRESET {
                        // peer closed the connection
                        println!("Connection reset by peer on socket {}", sock);
                        assert_eq!(cage.close_syscall(sock as i32), 0);
                        master_set.clear(sock);
                        master_outputs_set.clear(sock);
                    }
                }
            }

            // check for writefds
            for sock in 0..FD_SET_MAX_FD {
                if !outputs.is_set(sock) {
                    continue;
                }
                if sock == filefd {
                    // regular file
                    let mut buf = sizecbuf(4);
                    assert_eq!(cage.read_syscall(sock as i32, buf.as_mut_ptr(), 4), 4);
                    assert_eq!(cbuf2str(&buf), "test");
                    master_outputs_set.clear(sock);
                } else {
                    //Data is sent out this socket, it's no longer ready for writing remove this
                    // socket from writefd's.
                    assert_eq!(cage.send_syscall(sock as i32, str2cbuf("test"), 4, 0), 4);
                    master_outputs_set.clear(sock);
                }
            }
        }
        assert_eq!(cage.close_syscall(serversockfd), 0);
        assert_eq!(cage.close_syscall(serversockfd_unix), 0);

        // let threads exit
        barrier_exit.wait();

        threadclient1.join().unwrap();
        threadclient2.join().unwrap();
        threadclient_unix.join().unwrap();
        thread_pipe.join().unwrap();

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_net_shutdown_bad_input() {
        // this test is used for testing shutdown with error input

        // acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        // unexist file descriptor error
        assert_eq!(
            cage.netshutdown_syscall(10, SHUT_RD),
            -(Errno::EBADF as i32)
        );

        // out of range file descriptor error
        assert_eq!(
            cage.netshutdown_syscall(-1, SHUT_RD),
            -(Errno::EBADF as i32)
        );

        let filefd = cage.open_syscall("/netshutdowntest.txt", O_CREAT | O_EXCL | O_RDWR, S_IRWXA);
        assert!(filefd > 0);
        // wrong fd type error
        assert_eq!(
            cage.netshutdown_syscall(filefd, SHUT_RD),
            -(Errno::ENOTSOCK as i32)
        );

        let sockfd = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        let port: u16 = generate_random_port();
        //binding to a socket
        let sockaddr = interface::SockaddrV4 {
            sin_family: AF_INET as u16,
            sin_port: port.to_be(),
            sin_addr: interface::V4Addr {
                s_addr: u32::from_ne_bytes([127, 0, 0, 1]),
            },
            padding: 0,
        };
        let socket = interface::GenSockaddr::V4(sockaddr); //127.0.0.1

        // socket not connect error
        // BUG: failed the test
        // assert_eq!(
        //     cage.netshutdown_syscall(sockfd, SHUT_RD),
        //     -(Errno::ENOTCONN as i32)
        // );

        assert_eq!(cage.bind_syscall(sockfd, &socket), 0);
        assert_eq!(cage.listen_syscall(sockfd, 10), 0);

        // wrong how argument error
        assert_eq!(
            cage.netshutdown_syscall(sockfd, 10),
            -(Errno::EINVAL as i32)
        );

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_net_shutdown_unix() {
        // this test is used for testing shutdown with UNIX socket

        // acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        let serverfd = cage.socket_syscall(AF_UNIX, SOCK_STREAM, 0);

        let serveraddr = interface::new_sockaddr_unix(AF_UNIX as u16, "server_shutdown".as_bytes());
        let serversocket = interface::GenSockaddr::Unix(serveraddr);

        assert_eq!(cage.bind_syscall(serverfd, &serversocket), 0);
        assert_eq!(cage.listen_syscall(serverfd, 10), 0);

        assert_eq!(cage.fork_syscall(2), 0);
        assert_eq!(cage.fork_syscall(3), 0);

        let barrier = Arc::new(Barrier::new(2));
        let barrier_clone = barrier.clone();

        let barrier_shut = Arc::new(Barrier::new(2));
        let barrier_shut_clone = barrier_shut.clone();

        let thread = interface::helper_thread(move || {
            let cage2 = interface::cagetable_getref(2);

            let fd = cage2.socket_syscall(AF_UNIX, SOCK_STREAM, 0);
            assert_eq!(cage2.connect_syscall(fd, &serversocket), 0);

            // first make sure send and recv are working before shutdown
            assert_eq!(cage2.send_syscall(fd, str2cbuf("client send"), 11, 0), 11);
            let mut buf = sizecbuf(11);
            assert_eq!(cage2.read_syscall(fd, buf.as_mut_ptr(), 11), 11);
            assert_eq!(cbuf2str(&buf), "server send");

            barrier_clone.wait();

            // now let's shutdown RD
            assert_eq!(cage2.netshutdown_syscall(fd, SHUT_RD), 0);

            // BUG: read after SHUT_RD should not raise ENOTCONN error
            // the desired behavior is to return end-of-file (0)
            // assert_eq!(cage2.read_syscall(fd, buf.as_mut_ptr(), 8), 0);

            barrier_shut_clone.wait();

            // write should succeed
            assert_eq!(
                cage2.send_syscall(fd, str2cbuf("before SHUT_WR"), 14, 0),
                14
            );
            assert_eq!(cage2.netshutdown_syscall(fd, SHUT_WR), 0);

            assert_eq!(cage2.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        });

        let mut sockgarbage = interface::GenSockaddr::V4(interface::SockaddrV4::default());
        let fd = cage.accept_syscall(serverfd, &mut sockgarbage);
        assert!(fd > 0);

        // first make sure send and recv are working
        let mut buf = sizecbuf(11);
        assert_eq!(cage.read_syscall(fd, buf.as_mut_ptr(), 11), 11);
        assert_eq!(cbuf2str(&buf), "client send");
        assert_eq!(cage.send_syscall(fd, str2cbuf("server send"), 11, 0), 11);

        assert_eq!(cage.send_syscall(fd, str2cbuf("shutdown"), 8, 0), 8);
        barrier.wait();
        barrier_shut.wait();
        // BUG: peer already closed RD, now subsequent write should fail with EPIPE
        // assert_ne!(cage.send_syscall(fd, str2cbuf("shutdown"), 8, 0), 8);

        // after SHUT_WR, once the peer application has read all outstanding data, it
        // will see end-of-file.
        let mut buf = sizecbuf(14);
        assert_eq!(cage.read_syscall(fd, buf.as_mut_ptr(), 14), 14);
        assert_eq!(cbuf2str(&buf), "before SHUT_WR");
        assert_eq!(cage.read_syscall(fd, buf.as_mut_ptr(), 14), 0);

        assert_eq!(cage.close_syscall(fd), 0);

        let barrier2 = Arc::new(Barrier::new(2));
        let barrier2_clone = barrier2.clone();

        let barrier2_shut = Arc::new(Barrier::new(2));
        let barrier2_shut_clone = barrier2_shut.clone();

        let thread2 = interface::helper_thread(move || {
            let cage3 = interface::cagetable_getref(3);

            let fd = cage3.socket_syscall(AF_UNIX, SOCK_STREAM, 0);
            assert_eq!(cage3.connect_syscall(fd, &serversocket), 0);

            // first make sure send and recv are working before shutdown
            assert_eq!(cage3.send_syscall(fd, str2cbuf("client send"), 11, 0), 11);
            let mut buf = sizecbuf(11);
            assert_eq!(cage3.read_syscall(fd, buf.as_mut_ptr(), 11), 11);
            assert_eq!(cbuf2str(&buf), "server send");

            barrier2_clone.wait();

            // now let's shutdown RD and WR
            assert_eq!(cage3.netshutdown_syscall(fd, SHUT_RDWR), 0);

            barrier2_shut_clone.wait();

            // now neither send nor recv should succeed
            assert_ne!(cage3.send_syscall(fd, str2cbuf("client send"), 11, 0), 11);
            // BUG: should not raise ENOTCONN error
            // the desired behavior is to return end-of-file (0)
            // let mut buf = sizecbuf(11);
            // assert_eq!(cage3.read_syscall(fd, buf.as_mut_ptr(), 11), 0);

            assert_eq!(cage3.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        });

        let fd = cage.accept_syscall(serverfd, &mut sockgarbage);
        assert!(fd > 0);

        // first make sure send and recv are working
        let mut buf = sizecbuf(11);
        assert_eq!(cage.read_syscall(fd, buf.as_mut_ptr(), 11), 11);
        assert_eq!(cbuf2str(&buf), "client send");
        assert_eq!(cage.send_syscall(fd, str2cbuf("server send"), 11, 0), 11);

        barrier2.wait();
        barrier2_shut.wait();

        // peer just shutdown RD and WR
        // now neither send nor recv should succeed
        // BUG: failed the test below
        // assert_ne!(cage.send_syscall(fd, str2cbuf("server send"), 11, 0), 11);

        // BUG: peer already shutdown, read should not block
        // let mut buf = sizecbuf(11);
        // assert_eq!(cage.read_syscall(fd, buf.as_mut_ptr(), 11), 0);

        thread.join().unwrap();
        thread2.join().unwrap();

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_net_shutdown_inet() {
        // this test is used for testing shutdown with INET socket

        // acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        let serverfd = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);

        let port: u16 = generate_random_port();
        //binding to a socket
        let sockaddr = interface::SockaddrV4 {
            sin_family: AF_INET as u16,
            sin_port: port.to_be(),
            sin_addr: interface::V4Addr {
                s_addr: u32::from_ne_bytes([127, 0, 0, 1]),
            },
            padding: 0,
        };
        let socket = interface::GenSockaddr::V4(sockaddr); //127.0.0.1

        assert_eq!(cage.bind_syscall(serverfd, &socket), 0);
        assert_eq!(cage.listen_syscall(serverfd, 10), 0);

        assert_eq!(cage.fork_syscall(2), 0);
        assert_eq!(cage.fork_syscall(3), 0);

        let barrier = Arc::new(Barrier::new(2));
        let barrier_clone = barrier.clone();

        let thread = interface::helper_thread(move || {
            let cage2 = interface::cagetable_getref(2);

            let fd = cage2.socket_syscall(AF_INET, SOCK_STREAM, 0);
            assert_eq!(cage2.connect_syscall(fd, &socket), 0);

            // first make sure send and recv are working before shutdown
            assert_eq!(cage2.send_syscall(fd, str2cbuf("client send"), 11, 0), 11);
            let mut buf = sizecbuf(11);
            assert_eq!(cage2.read_syscall(fd, buf.as_mut_ptr(), 11), 11);
            assert_eq!(cbuf2str(&buf), "server send");

            assert_eq!(
                cage2.send_syscall(fd, str2cbuf("before SHUT_WR"), 14, 0),
                14
            );
            assert_eq!(cage2.netshutdown_syscall(fd, SHUT_WR), 0);

            barrier_clone.wait();

            assert_eq!(cage2.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        });

        let mut sockgarbage = interface::GenSockaddr::V4(interface::SockaddrV4::default());
        let fd = cage.accept_syscall(serverfd, &mut sockgarbage);
        assert!(fd > 0);

        // first make sure send and recv are working
        let mut buf = sizecbuf(11);
        assert_eq!(cage.read_syscall(fd, buf.as_mut_ptr(), 11), 11);
        assert_eq!(cbuf2str(&buf), "client send");
        assert_eq!(cage.send_syscall(fd, str2cbuf("server send"), 11, 0), 11);

        // peer SHUT_WR
        barrier.wait();
        let mut buf = sizecbuf(14);
        // data already sent should still be readable
        assert_eq!(cage.read_syscall(fd, buf.as_mut_ptr(), 14), 14);
        assert_eq!(cbuf2str(&buf), "before SHUT_WR");
        // subsequent read should return end-of-file
        assert_eq!(cage.read_syscall(fd, buf.as_mut_ptr(), 14), 0);

        assert_eq!(cage.close_syscall(fd), 0);

        let barrier2 = Arc::new(Barrier::new(2));
        let barrier2_clone = barrier2.clone();

        let thread2 = interface::helper_thread(move || {
            let cage3 = interface::cagetable_getref(3);

            let fd = cage3.socket_syscall(AF_INET, SOCK_STREAM, 0);
            assert_eq!(cage3.connect_syscall(fd, &socket), 0);

            // first make sure send and recv are working before shutdown
            assert_eq!(cage3.send_syscall(fd, str2cbuf("client send"), 11, 0), 11);
            let mut buf = sizecbuf(11);
            assert_eq!(cage3.read_syscall(fd, buf.as_mut_ptr(), 11), 11);
            assert_eq!(cbuf2str(&buf), "server send");

            assert_eq!(
                cage3.send_syscall(fd, str2cbuf("before SHUT_RDWR"), 16, 0),
                16
            );
            assert_eq!(cage3.netshutdown_syscall(fd, SHUT_RDWR), 0);

            barrier2_clone.wait();

            assert_eq!(cage3.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        });

        let fd = cage.accept_syscall(serverfd, &mut sockgarbage);
        assert!(fd > 0);

        // first make sure send and recv are working
        let mut buf = sizecbuf(11);
        assert_eq!(cage.read_syscall(fd, buf.as_mut_ptr(), 11), 11);
        assert_eq!(cbuf2str(&buf), "client send");
        assert_eq!(cage.send_syscall(fd, str2cbuf("server send"), 11, 0), 11);

        // peer SHUT_RDWR
        barrier2.wait();
        let mut buf = sizecbuf(16);
        // data already sent should still be readable
        assert_eq!(cage.read_syscall(fd, buf.as_mut_ptr(), 16), 16);
        assert_eq!(cbuf2str(&buf), "before SHUT_RDWR");
        // subsequent read should return end-of-file
        assert_eq!(cage.read_syscall(fd, buf.as_mut_ptr(), 16), 0);

        thread.join().unwrap();
        thread2.join().unwrap();

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_net_shutdown() {
        // acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        let serversockfd = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        let clientsockfd = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);

        assert!(serversockfd > 0);
        assert!(clientsockfd > 0);
        let port: u16 = generate_random_port();

        //binding to a socket
        let sockaddr = interface::SockaddrV4 {
            sin_family: AF_INET as u16,
            sin_port: port.to_be(),
            sin_addr: interface::V4Addr {
                s_addr: u32::from_ne_bytes([127, 0, 0, 1]),
            },
            padding: 0,
        };
        let socket = interface::GenSockaddr::V4(sockaddr); //127.0.0.1
        assert_eq!(cage.bind_syscall(serversockfd, &socket), 0);
        assert_eq!(cage.listen_syscall(serversockfd, 10), 0);

        //forking the cage to get another cage with the same information
        assert_eq!(cage.fork_syscall(2), 0);

        let thread = interface::helper_thread(move || {
            let cage2 = interface::cagetable_getref(2);

            interface::sleep(interface::RustDuration::from_millis(100));
            let port: u16 = generate_random_port();

            let mut socket2 = interface::GenSockaddr::V4(interface::SockaddrV4 {
                sin_family: AF_INET as u16,
                sin_port: port.to_be(),
                sin_addr: interface::V4Addr {
                    s_addr: u32::from_ne_bytes([127, 0, 0, 1]),
                },
                padding: 0,
            }); //127.0.0.1
            let fd = cage2.accept_syscall(serversockfd, &mut socket2);
            assert!(fd > 0);

            assert_eq!(cage2.netshutdown_syscall(fd, SHUT_RD), 0);
            assert_eq!(cage2.send_syscall(fd, str2cbuf("random string"), 13, 0), 13);
            assert_eq!(cage2.netshutdown_syscall(fd, SHUT_RDWR), 0);

            assert_eq!(cage2.close_syscall(serversockfd), 0);
            assert_eq!(cage2.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        });

        assert_eq!(cage.connect_syscall(clientsockfd, &socket), 0);

        let mut retsocket = interface::GenSockaddr::V4(interface::SockaddrV4::default());
        assert_eq!(cage.getsockname_syscall(clientsockfd, &mut retsocket), 0);
        assert_ne!(retsocket, socket);

        thread.join().unwrap();

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_net_socket() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        // Following checks are inplace to ensure that the socket types are correctly
        // defined and that the assumptions about the values of the socket types
        // are correct across platforms. Check that SOCK_STREAM only uses the
        // lowest 3 bits
        assert_eq!(SOCK_STREAM & !0x7, 0);

        // Check that SOCK_DGRAM only uses the lowest 3 bits
        assert_eq!(SOCK_DGRAM & !0x7, 0);

        // Check that SOCK_NONBLOCK does not use the lowest 3 bits
        assert_eq!(SOCK_NONBLOCK & 0x7, 0);

        // Check that SOCK_CLOEXEC does not use the lowest 3 bits
        assert_eq!(SOCK_CLOEXEC & 0x7, 0);

        //let's check an illegal operation...
        // RDM is not a valid socket type for SOCK_DGRAM (UDP) as its not implemented
        // yet.
        let sockfd5 = cage.socket_syscall(AF_INET, SOCK_RDM, 0);
        assert!(
            sockfd5 < 0,
            "Expected an error, got a valid file descriptor"
        );

        //let's check an illegal operation...
        //invalid protocol for SOCK_STREAM Type.
        let sockfd6 = cage.socket_syscall(AF_INET, SOCK_STREAM, 999);
        assert!(
            sockfd6 < 0,
            "Expected an error, got a valid file descriptor"
        );

        //let's check an illegal operation...
        //invalid domain for socket
        let sockfd7 = cage.socket_syscall(999, SOCK_STREAM, 0);
        assert!(
            sockfd7 < 0,
            "Expected an error, got a valid file descriptor"
        );

        //let's check an illegal operation...
        //invalid socket type flags combination
        let sockfd8 = cage.socket_syscall(AF_INET, SOCK_STREAM | 0x100000, 0);
        assert!(
            sockfd8 < 0,
            "Expected an error, got a valid file descriptor"
        );

        //let's check an illegal operation...
        //invalid socket type protocol combination
        let sockfd9 = cage.socket_syscall(AF_INET, SOCK_STREAM, IPPROTO_UDP);
        assert!(
            sockfd9 < 0,
            "Expected an error, got a valid file descriptor"
        );

        //let's check an illegal operation...
        //invalid socket type protocol combination
        let sockfd10 = cage.socket_syscall(AF_INET, SOCK_DGRAM, IPPROTO_TCP);
        assert!(
            sockfd10 < 0,
            "Expected an error, got a valid file descriptor"
        );

        let mut sockfd = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        assert!(sockfd > 0, "Expected a valid file descriptor, got error");

        let sockfd2 = cage.socket_syscall(AF_INET, SOCK_STREAM, IPPROTO_TCP);
        assert!(sockfd2 > 0, "Expected a valid file descriptor, got error");

        let sockfd3 = cage.socket_syscall(AF_INET, SOCK_DGRAM, 0);
        assert!(sockfd3 > 0, "Expected a valid file descriptor, got error");

        let sockfd4 = cage.socket_syscall(AF_INET, SOCK_DGRAM, IPPROTO_UDP);
        assert!(sockfd4 > 0, "Expected a valid file descriptor, got error");

        //let's check an illegal operation...
        // let sockfd7 = cage.socket_syscall(AF_INET, !0b111 | 0b001, 0);
        // assert!(sockfd7 < 0, "Expected an error, got a valid file descriptor");

        let sockfddomain = cage.socket_syscall(AF_UNIX, SOCK_DGRAM, 0);
        assert!(
            sockfddomain > 0,
            "Expected a valid file descriptor, got error"
        );

        sockfd = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        assert!(sockfd > 0, "Expected a valid file descriptor, got error");

        assert_eq!(
            cage.close_syscall(sockfd),
            0,
            "Expected successful close, got error"
        );
        assert_eq!(
            cage.exit_syscall(EXIT_SUCCESS),
            EXIT_SUCCESS,
            "Expected successful exit, got error"
        );
        lindrustfinalize();
    }

    #[test]
    #[ignore]
    pub fn ut_lind_net_sockopt_bad_input_optname() {
        // this test is used for testing invalid optname that is
        // large enough to overflow to bitwise shift
        // would fail currently

        // acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        let sockfd = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        let mut optstore = 0;

        assert_eq!(
            cage.getsockopt_syscall(sockfd, SOL_TCP, 100, &mut optstore),
            -(Errno::ENOPROTOOPT as i32)
        );
        assert_eq!(
            cage.getsockopt_syscall(sockfd, SOL_SOCKET, 100, &mut optstore),
            -(Errno::ENOPROTOOPT as i32)
        );

        assert_eq!(
            cage.setsockopt_syscall(sockfd, SOL_TCP, 100, 0),
            -(Errno::EOPNOTSUPP as i32)
        );
        assert_eq!(
            cage.setsockopt_syscall(sockfd, SOL_SOCKET, 100, 0),
            -(Errno::EOPNOTSUPP as i32)
        );

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_net_getsockopt_bad_input() {
        // this test is used for testing getsockopt_syscall with error input

        // acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        let filefd =
            cage.open_syscall("/netgetsockopttest.txt", O_CREAT | O_EXCL | O_RDWR, S_IRWXA);
        let sockfd = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);

        let mut optstore = -12;

        // unexist file descriptor
        assert_eq!(
            cage.getsockopt_syscall(10, SOL_SOCKET, SO_REUSEPORT, &mut optstore),
            -(Errno::EBADF as i32)
        );

        // out of range file descriptor
        assert_eq!(
            cage.getsockopt_syscall(-1, SOL_SOCKET, SO_REUSEPORT, &mut optstore),
            -(Errno::EBADF as i32)
        );

        // fd that is not socket
        assert_eq!(
            cage.getsockopt_syscall(filefd, SOL_SOCKET, SO_REUSEPORT, &mut optstore),
            -(Errno::ENOTSOCK as i32)
        );

        // invalid level argument
        assert_eq!(
            cage.getsockopt_syscall(sockfd, 30, SO_REUSEPORT, &mut optstore),
            -(Errno::EOPNOTSUPP as i32)
        );

        // unsupported level argument
        assert_eq!(
            cage.getsockopt_syscall(sockfd, SOL_UDP, SO_REUSEPORT, &mut optstore),
            -(Errno::ENOPROTOOPT as i32)
        );

        // invalid optname argument
        assert_eq!(
            cage.getsockopt_syscall(sockfd, SOL_SOCKET, 20, &mut optstore),
            -(Errno::ENOPROTOOPT as i32)
        );
        assert_eq!(
            cage.getsockopt_syscall(sockfd, SOL_TCP, 20, &mut optstore),
            -(Errno::ENOPROTOOPT as i32)
        );

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_net_setsockopt_bad_input() {
        // this test is used for testing setsockopt_syscall with error input

        // acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        let filefd =
            cage.open_syscall("/netsetsockopttest.txt", O_CREAT | O_EXCL | O_RDWR, S_IRWXA);
        let sockfd = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);

        // unexist file descriptor
        assert_eq!(
            cage.setsockopt_syscall(10, SOL_SOCKET, SO_REUSEPORT, 0),
            -(Errno::EBADF as i32)
        );

        // out of range file descriptor
        assert_eq!(
            cage.setsockopt_syscall(-1, SOL_SOCKET, SO_REUSEPORT, 0),
            -(Errno::EBADF as i32)
        );

        // fd that is not socket
        assert_eq!(
            cage.setsockopt_syscall(filefd, SOL_SOCKET, SO_REUSEPORT, 0),
            -(Errno::ENOTSOCK as i32)
        );

        // invalid level argument
        assert_eq!(
            cage.setsockopt_syscall(sockfd, 30, SO_REUSEPORT, 0),
            -(Errno::EOPNOTSUPP as i32)
        );

        // unsupported level argument
        assert_eq!(
            cage.setsockopt_syscall(sockfd, SOL_UDP, SO_REUSEPORT, 0),
            -(Errno::EOPNOTSUPP as i32)
        );

        // invalid optname argument
        assert_eq!(
            cage.setsockopt_syscall(sockfd, SOL_SOCKET, 20, 0),
            -(Errno::EOPNOTSUPP as i32)
        );
        assert_eq!(
            cage.setsockopt_syscall(sockfd, SOL_TCP, 20, 0),
            -(Errno::EOPNOTSUPP as i32)
        );

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_net_socketoptions() {
        // acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        let sockfd = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        assert!(sockfd > 0);
        let port: u16 = generate_random_port();

        let sockaddr = interface::SockaddrV4 {
            sin_family: AF_INET as u16,
            sin_port: port.to_be(),
            sin_addr: interface::V4Addr {
                s_addr: u32::from_ne_bytes([127, 0, 0, 1]),
            },
            padding: 0,
        };
        let socket = interface::GenSockaddr::V4(sockaddr); //127.0.0.1
        assert_eq!(cage.bind_syscall(sockfd, &socket), 0);
        assert_eq!(cage.listen_syscall(sockfd, 4), 0);

        // set and get some options:
        let mut optstore = -12;
        assert_eq!(
            cage.getsockopt_syscall(sockfd, SOL_SOCKET, SO_REUSEPORT, &mut optstore),
            0
        );
        assert_eq!(optstore, 0);
        assert_eq!(
            cage.getsockopt_syscall(sockfd, SOL_SOCKET, SO_LINGER, &mut optstore),
            0
        );
        assert_eq!(optstore, 0);
        assert_eq!(
            cage.getsockopt_syscall(sockfd, SOL_SOCKET, SO_KEEPALIVE, &mut optstore),
            0
        );
        assert_eq!(optstore, 0);

        // linger...
        assert_eq!(
            cage.getsockopt_syscall(sockfd, SOL_SOCKET, SO_LINGER, &mut optstore),
            0
        );
        assert_eq!(optstore, 0);
        assert_eq!(cage.setsockopt_syscall(sockfd, SOL_SOCKET, SO_LINGER, 1), 0);
        assert_eq!(
            cage.getsockopt_syscall(sockfd, SOL_SOCKET, SO_LINGER, &mut optstore),
            0
        );
        assert_eq!(optstore, 1);

        // check the options
        assert_eq!(
            cage.getsockopt_syscall(sockfd, SOL_SOCKET, SO_REUSEPORT, &mut optstore),
            0
        );
        assert_eq!(optstore, 0);
        assert_eq!(
            cage.getsockopt_syscall(sockfd, SOL_SOCKET, SO_LINGER, &mut optstore),
            0
        );
        assert_eq!(optstore, 1);
        assert_eq!(
            cage.getsockopt_syscall(sockfd, SOL_SOCKET, SO_KEEPALIVE, &mut optstore),
            0
        );
        assert_eq!(optstore, 0);

        // reuseport...
        assert_eq!(
            cage.getsockopt_syscall(sockfd, SOL_SOCKET, SO_REUSEPORT, &mut optstore),
            0
        );
        assert_eq!(optstore, 0);
        assert_eq!(
            cage.setsockopt_syscall(sockfd, SOL_SOCKET, SO_REUSEPORT, 1),
            0
        );
        assert_eq!(
            cage.getsockopt_syscall(sockfd, SOL_SOCKET, SO_REUSEPORT, &mut optstore),
            0
        );
        assert_eq!(optstore, 1);

        // check the options
        assert_eq!(
            cage.getsockopt_syscall(sockfd, SOL_SOCKET, SO_REUSEPORT, &mut optstore),
            0
        );
        assert_eq!(optstore, 1);
        assert_eq!(
            cage.getsockopt_syscall(sockfd, SOL_SOCKET, SO_LINGER, &mut optstore),
            0
        );
        assert_eq!(optstore, 1);
        assert_eq!(
            cage.getsockopt_syscall(sockfd, SOL_SOCKET, SO_KEEPALIVE, &mut optstore),
            0
        );
        assert_eq!(optstore, 0);

        // keep alive...
        assert_eq!(
            cage.getsockopt_syscall(sockfd, SOL_SOCKET, SO_KEEPALIVE, &mut optstore),
            0
        );
        assert_eq!(optstore, 0);
        assert_eq!(
            cage.setsockopt_syscall(sockfd, SOL_SOCKET, SO_KEEPALIVE, 1),
            0
        );

        // check the options
        assert_eq!(
            cage.getsockopt_syscall(sockfd, SOL_SOCKET, SO_REUSEPORT, &mut optstore),
            0
        );
        assert_eq!(optstore, 1);
        assert_eq!(
            cage.getsockopt_syscall(sockfd, SOL_SOCKET, SO_LINGER, &mut optstore),
            0
        );
        assert_eq!(optstore, 1);
        assert_eq!(
            cage.getsockopt_syscall(sockfd, SOL_SOCKET, SO_KEEPALIVE, &mut optstore),
            0
        );
        assert_eq!(optstore, 1);

        assert_eq!(
            cage.setsockopt_syscall(sockfd, SOL_SOCKET, SO_SNDBUF, 1000),
            0
        );
        assert_eq!(
            cage.getsockopt_syscall(sockfd, SOL_SOCKET, SO_SNDBUF, &mut optstore),
            0
        );
        assert_eq!(optstore, 1000);

        assert_eq!(
            cage.setsockopt_syscall(sockfd, SOL_SOCKET, SO_RCVBUF, 2000),
            0
        );
        assert_eq!(
            cage.getsockopt_syscall(sockfd, SOL_SOCKET, SO_RCVBUF, &mut optstore),
            0
        );
        assert_eq!(optstore, 2000);

        // check the options
        assert_eq!(
            cage.getsockopt_syscall(sockfd, SOL_SOCKET, SO_REUSEPORT, &mut optstore),
            0
        );
        assert_eq!(optstore, 1);
        assert_eq!(
            cage.getsockopt_syscall(sockfd, SOL_SOCKET, SO_LINGER, &mut optstore),
            0
        );
        assert_eq!(optstore, 1);
        assert_eq!(
            cage.getsockopt_syscall(sockfd, SOL_SOCKET, SO_KEEPALIVE, &mut optstore),
            0
        );
        assert_eq!(optstore, 1);

        // TCP_NODELAY for SOL_TCP
        assert_eq!(cage.setsockopt_syscall(sockfd, SOL_TCP, TCP_NODELAY, 1), 0);
        assert_eq!(
            cage.getsockopt_syscall(sockfd, SOL_TCP, TCP_NODELAY, &mut optstore),
            0
        );
        assert_eq!(optstore, 1);

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_net_socketpair() {
        // this test is used for testing a generic use case of socketpair
        // test involves creating a TCP socketpair let two threads communicate
        // with the socketpair

        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();
        let cage = interface::cagetable_getref(1);
        let mut socketpair = interface::SockPair::default();
        assert_eq!(
            Cage::socketpair_syscall(cage.clone(), AF_UNIX, SOCK_STREAM, 0, &mut socketpair),
            0
        );
        let cage2 = cage.clone();

        let thread = interface::helper_thread(move || {
            // this thread first receives the message, then send the message
            let mut buf = sizecbuf(10);
            loop {
                let result = cage2.recv_syscall(socketpair.sock2, buf.as_mut_ptr(), 10, 0);
                if result != -libc::EINTR {
                    break; // if the error was EINTR, retry the syscall
                }
            }
            // check if received message is correct
            assert_eq!(cbuf2str(&buf), "test\0\0\0\0\0\0");

            // send message to peer
            assert_eq!(
                cage2.send_syscall(socketpair.sock2, str2cbuf("Socketpair Test"), 15, 0),
                15
            );
        });

        let cage3 = cage.clone();
        let thread_2 = interface::helper_thread(move || {
            // this thread first send the message, then receive the message
            assert_eq!(
                cage3.send_syscall(socketpair.sock1, str2cbuf("test"), 4, 0),
                4
            );

            let mut buf2 = sizecbuf(15);
            loop {
                let result = cage3.recv_syscall(socketpair.sock1, buf2.as_mut_ptr(), 15, 0);
                if result != -libc::EINTR {
                    break; // if the error was EINTR, retry the syscall
                }
            }
            let str2 = cbuf2str(&buf2);
            // check if received message is correct
            assert_eq!(str2, "Socketpair Test");
        });

        thread.join().unwrap();
        thread_2.join().unwrap();

        assert_eq!(cage.close_syscall(socketpair.sock1), 0);
        assert_eq!(cage.close_syscall(socketpair.sock2), 0);

        // end of the socket pair test (note we are only supporting AF_UNIX and TCP)

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_net_socketpair_bad_input() {
        // test for error cases of socketpair

        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();
        let cage = interface::cagetable_getref(1);
        let mut socketpair = interface::SockPair::default();

        // test for unsupported domain
        // socketpair only works with AF_UNIX
        assert_eq!(
            Cage::socketpair_syscall(cage.clone(), AF_INET, SOCK_STREAM, 0, &mut socketpair),
            -(Errno::EOPNOTSUPP as i32)
        );
        assert_eq!(
            Cage::socketpair_syscall(cage.clone(), AF_INET6, SOCK_STREAM, 0, &mut socketpair),
            -(Errno::EOPNOTSUPP as i32)
        );

        // test for unsupported socktype
        // socketpair only works with SOCK_STREAM
        assert_eq!(
            Cage::socketpair_syscall(cage.clone(), AF_UNIX, SOCK_DGRAM, 0, &mut socketpair),
            -(Errno::EOPNOTSUPP as i32)
        );

        // test for unsupported protocol
        // we only support for protocol of 0
        assert_eq!(
            Cage::socketpair_syscall(cage.clone(), AF_UNIX, SOCK_STREAM, 1, &mut socketpair),
            -(Errno::EOPNOTSUPP as i32)
        );

        // test for bad structured input
        assert_eq!(
            Cage::socketpair_syscall(cage.clone(), AF_UNIX, 472810394, 0, &mut socketpair),
            -(Errno::EOPNOTSUPP as i32)
        );

        // test for invalid flags
        assert_eq!(
            Cage::socketpair_syscall(
                cage.clone(),
                AF_UNIX,
                SOCK_STREAM | 1024,
                0,
                &mut socketpair
            ),
            -(Errno::EINVAL as i32)
        );

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_net_socketpair_cloexec() {
        // this test is used for testing socketpair when cloexec flag is set
        // when cloexec flag is set, the file descriptor of the socket should
        // be automatically closed when exec_syscall is called

        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();
        let cage = interface::cagetable_getref(1);
        let mut socketpair = interface::SockPair::default();
        // try with cloexec flag
        assert_eq!(
            Cage::socketpair_syscall(
                cage.clone(),
                AF_UNIX,
                SOCK_STREAM | SOCK_CLOEXEC,
                0,
                &mut socketpair
            ),
            0
        );

        // we use fstat_syscall to inspect if the file descriptor is valid
        let mut uselessstatdata = StatData::default();

        // check if the file descriptor exists
        // if the file descriptor does not exist, it should return another error
        assert_eq!(
            cage.fstat_syscall(socketpair.sock1, &mut uselessstatdata),
            -(Errno::EOPNOTSUPP as i32)
        );
        assert_eq!(
            cage.fstat_syscall(socketpair.sock2, &mut uselessstatdata),
            -(Errno::EOPNOTSUPP as i32)
        );

        // now exec the cage
        assert_eq!(cage.exec_syscall(2), 0);

        // check if the file descriptor is closed in new cage
        // EBADF is the error that is supposed to be returned when file descriptor does
        // not exist
        let newcage = interface::cagetable_getref(2);
        assert_eq!(
            newcage.fstat_syscall(socketpair.sock1, &mut uselessstatdata),
            -(Errno::EBADF as i32)
        );
        assert_eq!(
            newcage.fstat_syscall(socketpair.sock2, &mut uselessstatdata),
            -(Errno::EBADF as i32)
        );

        assert_eq!(newcage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_net_socketpair_nonblocking() {
        // this test is used for testing socketpair when nonblocking flag is set
        // when nonblocking flag is set, the socket should not block on syscalls like
        // recv_syscall, instead, EAGAIN error should be returned when there is no
        // data to receive

        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();
        let cage = interface::cagetable_getref(1);
        let mut socketpair = interface::SockPair::default();

        // try with nonblocking flag
        assert_eq!(
            Cage::socketpair_syscall(
                cage.clone(),
                AF_UNIX,
                SOCK_STREAM | SOCK_NONBLOCK,
                0,
                &mut socketpair
            ),
            0
        );

        let cage2 = cage.clone();

        let thread = interface::helper_thread(move || {
            let mut buf = sizecbuf(10);
            // counter is used for recording how many times do recv_syscall returned
            // with EAGAIN
            let mut counter = 0;

            // receive the message
            // since peer will sleep for 30ms before send the message
            // some nonblocking returns are expected
            loop {
                let result = cage2.recv_syscall(socketpair.sock2, buf.as_mut_ptr(), 10, 0);
                if result == -(Errno::EAGAIN as i32) {
                    // return due to nonblocking flag
                    counter += 1;
                    continue;
                }
                if result != -libc::EINTR {
                    break; // if the error was EINTR, retry the syscall
                }
            }
            // check if the received message is correct
            assert_eq!(cbuf2str(&buf), "test\0\0\0\0\0\0");
            // check if there is any nonblocking return
            assert_ne!(counter, 0);

            // sleep for 30ms so receiver could have some nonblocking return
            interface::sleep(interface::RustDuration::from_millis(30));
            // send the message
            assert_eq!(
                cage2.send_syscall(socketpair.sock2, str2cbuf("Socketpair Test"), 15, 0),
                15
            );
        });

        let cage3 = cage.clone();

        let thread_2 = interface::helper_thread(move || {
            // sleep for 30ms so receiver could have some nonblocking return
            interface::sleep(interface::RustDuration::from_millis(30));

            // send message
            assert_eq!(
                cage3.send_syscall(socketpair.sock1, str2cbuf("test"), 4, 0),
                4
            );

            // receive the message
            // since peer will sleep for 30ms before send the message
            // some nonblocking returns are expected
            let mut buf2 = sizecbuf(15);
            let mut counter = 0;
            loop {
                let result = cage3.recv_syscall(socketpair.sock1, buf2.as_mut_ptr(), 15, 0);
                if result == -(Errno::EAGAIN as i32) {
                    // return due to nonblocking flag
                    counter += 1;
                    continue;
                }
                if result != -libc::EINTR {
                    break; // if the error was EINTR, retry the syscall
                }
            }
            // check if the received message is correct
            let str2 = cbuf2str(&buf2);
            assert_eq!(str2, "Socketpair Test");
            // check if there is any nonblocking return
            assert_ne!(counter, 0);
        });

        thread.join().unwrap();
        thread_2.join().unwrap();

        assert_eq!(cage.close_syscall(socketpair.sock1), 0);
        assert_eq!(cage.close_syscall(socketpair.sock2), 0);

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_net_udp_bad_bind() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        let sockfd = cage.socket_syscall(AF_INET, SOCK_DGRAM, 0);
        assert!(sockfd > 0); //checking that the sockfd is valid
        let port: u16 = generate_random_port();

        let sockaddr = interface::SockaddrV4 {
            sin_family: AF_INET as u16,
            sin_port: port.to_be(),
            sin_addr: interface::V4Addr {
                s_addr: u32::from_ne_bytes([127, 0, 0, 1]),
            },
            padding: 0,
        };
        let socket = interface::GenSockaddr::V4(sockaddr); //127.0.0.1
        let port: u16 = generate_random_port();

        let _sockaddr2 = interface::SockaddrV4 {
            sin_family: AF_INET as u16,
            sin_port: port.to_be(),
            sin_addr: interface::V4Addr {
                s_addr: u32::from_ne_bytes([127, 0, 0, 1]),
            },
            padding: 0,
        };
        let socket2 = interface::GenSockaddr::V4(sockaddr); //127.0.0.1

        assert_eq!(cage.bind_syscall(sockfd, &socket), 0);
        assert_eq!(cage.connect_syscall(sockfd, &socket2), 0);

        //now the bind should fail...
        assert_ne!(cage.bind_syscall(sockfd, &socket), 0);

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_net_udp_simple() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        //just going to test the basic connect with UDP now...
        let serverfd = cage.socket_syscall(AF_INET, SOCK_DGRAM, 0);
        let clientfd = cage.socket_syscall(AF_INET, SOCK_DGRAM, 0);
        let port: u16 = generate_random_port();

        let socket = interface::GenSockaddr::V4(interface::SockaddrV4 {
            sin_family: AF_INET as u16,
            sin_port: port.to_be(),
            sin_addr: interface::V4Addr {
                s_addr: u32::from_ne_bytes([127, 0, 0, 1]),
            },
            padding: 0,
        });

        assert!(serverfd > 0);
        assert!(clientfd > 0);

        //forking the cage to get another cage with the same information
        assert_eq!(cage.fork_syscall(2), 0);
        let thread = interface::helper_thread(move || {
            let cage2 = interface::cagetable_getref(2);
            assert_eq!(cage2.bind_syscall(serverfd, &socket), 0);

            interface::sleep(interface::RustDuration::from_millis(30));

            let mut buf = sizecbuf(10);
            loop {
                let result = cage2.recv_syscall(serverfd, buf.as_mut_ptr(), 10, 0);
                if result != -libc::EINTR {
                    break; // if the error was EINTR, retry the syscall
                }
            }
            assert_eq!(cbuf2str(&buf), "test\0\0\0\0\0\0");

            interface::sleep(interface::RustDuration::from_millis(30));
            loop {
                let result = cage2.recv_syscall(serverfd, buf.as_mut_ptr(), 10, 0);
                if result != -libc::EINTR {
                    assert_eq!(result, 5);
                    break; // if the error was EINTR, retry the syscall
                }
            }
            assert_eq!(cbuf2str(&buf), "test2\0\0\0\0\0");

            assert_eq!(cage2.close_syscall(serverfd), 0);
            assert_eq!(cage2.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        });

        interface::sleep(interface::RustDuration::from_millis(50));
        let mut buf2 = str2cbuf("test");
        assert_eq!(cage.sendto_syscall(clientfd, buf2, 4, 0, &socket), 4);
        let sendsockfd2 = cage.socket_syscall(AF_INET, SOCK_DGRAM, 0);
        assert!(sendsockfd2 > 0);
        let port: u16 = generate_random_port();

        let sockaddr2 = interface::SockaddrV4 {
            sin_family: AF_INET as u16,
            sin_port: port.to_be(),
            sin_addr: interface::V4Addr {
                s_addr: u32::from_ne_bytes([127, 0, 0, 1]),
            },
            padding: 0,
        };
        let socket2 = interface::GenSockaddr::V4(sockaddr2); //127.0.0.1

        interface::sleep(interface::RustDuration::from_millis(50));

        buf2 = str2cbuf("test2");
        assert_eq!(cage.bind_syscall(sendsockfd2, &socket2), 0);
        assert_eq!(cage.sendto_syscall(sendsockfd2, buf2, 5, 0, &socket), 5);

        thread.join().unwrap();

        assert_eq!(cage.close_syscall(sendsockfd2), 0);
        assert_eq!(cage.close_syscall(clientfd), 0);
        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_net_udp_connect() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        //getting the sockets set up...
        let listenfd = cage.socket_syscall(AF_INET, SOCK_DGRAM, 0);
        let sendfd = cage.socket_syscall(AF_INET, SOCK_DGRAM, 0);
        let port: u16 = generate_random_port();
        let sockaddr = interface::SockaddrV4 {
            sin_family: AF_INET as u16,
            sin_port: port.to_be(),
            sin_addr: interface::V4Addr {
                s_addr: u32::from_ne_bytes([127, 0, 0, 1]),
            },
            padding: 0,
        };
        let socket = interface::GenSockaddr::V4(sockaddr); //127.0.0.1

        assert!(listenfd > 0);
        assert!(sendfd > 0);

        assert_eq!(cage.bind_syscall(listenfd, &socket), 0);

        //forking the cage to get another cage with the same information
        assert_eq!(cage.fork_syscall(2), 0);

        let thread = interface::helper_thread(move || {
            let cage2 = interface::cagetable_getref(2);

            interface::sleep(interface::RustDuration::from_millis(20));
            let mut buf = sizecbuf(16);
            loop {
                let result = cage2.recv_syscall(listenfd, buf.as_mut_ptr(), 16, 0);
                if result != -libc::EINTR {
                    assert_eq!(result, 16);
                    break; // if the error was EINTR, retry the syscall
                }
            }
            assert_ne!(buf, sizecbuf(16));
            assert_eq!(cbuf2str(&buf), "UDP Connect Test");

            assert_eq!(cage2.close_syscall(listenfd), 0);
            assert_eq!(cage2.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        });

        assert_eq!(cage.connect_syscall(sendfd, &socket), 0);
        interface::sleep(interface::RustDuration::from_millis(50));
        assert_eq!(
            cage.send_syscall(sendfd, str2cbuf("UDP Connect Test"), 16, 0),
            16
        );
        thread.join().unwrap();

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_net_gethostname() {
        // acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        // Assuming DEFAULT_HOSTNAME == "Lind" and change of hostname is not allowed

        let cage = interface::cagetable_getref(1);

        let mut buf = vec![0u8; 5];
        let bufptr: *mut u8 = &mut buf[0];
        assert_eq!(
            cage.gethostname_syscall(bufptr, -1),
            -(Errno::EINVAL as i32)
        );
        assert_eq!(cage.gethostname_syscall(bufptr, 5), 0);
        assert_eq!(std::str::from_utf8(&buf).unwrap(), "Lind\0");

        let mut buf = vec![0u8; 5];
        let bufptr: *mut u8 = &mut buf[0];
        assert_eq!(cage.gethostname_syscall(bufptr, 4), 0);
        assert_eq!(std::str::from_utf8(&buf).unwrap(), "Lind\0");

        let mut buf = vec![0u8; 5];
        let bufptr: *mut u8 = &mut buf[0];
        assert_eq!(cage.gethostname_syscall(bufptr, 2), 0);
        assert_eq!(std::str::from_utf8(&buf).unwrap(), "Li\0\0\0");

        let mut buf = vec![0u8; 4];
        let bufptr: *mut u8 = &mut buf[0];
        assert_eq!(cage.gethostname_syscall(bufptr, 4), 0);
        assert_eq!(std::str::from_utf8(&buf).unwrap(), "Lind");

        let mut buf = vec![0u8; 2];
        let bufptr: *mut u8 = &mut buf[0];
        assert_eq!(cage.gethostname_syscall(bufptr, 2), 0);
        assert_eq!(std::str::from_utf8(&buf).unwrap(), "Li");

        let mut buf = vec![0u8; 2];
        let bufptr: *mut u8 = &mut buf[0];
        assert_eq!(cage.gethostname_syscall(bufptr, 0), 0);
        assert_eq!(std::str::from_utf8(&buf).unwrap(), "\0\0");

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_net_getifaddrs() {
        // acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        // get the address
        let mut buf = vec![0u8; 200];
        let bufptr: *mut u8 = &mut buf[0];
        // not enough space would cause EOPNOTSUPP error
        assert_eq!(
            cage.getifaddrs_syscall(bufptr, 1),
            -(Errno::EOPNOTSUPP as i32)
        );
        assert_eq!(cage.getifaddrs_syscall(bufptr, 200), 0);
        // split the address string into vector
        let result = std::str::from_utf8(&buf).unwrap().replace("\0", "");
        let addrs = result.trim().split("\n").collect::<Vec<&str>>();
        // we should have some addresses returned
        assert!(addrs.len() > 0);

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_net_dns_rootserver_ping() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        //https://w3.cs.jmu.edu/kirkpams/OpenCSF/Books/csf/html/UDPSockets.html
        #[repr(C)]
        struct DnsHeader {
            xid: u16,
            flags: u16,
            qdcount: u16,
            ancount: u16,
            nscount: u16,
            arcount: u16,
        }

        /* Structure of the bytes for an IPv4 answer */
        #[repr(C, packed(1))]
        struct DnsRecordAT {
            compression: u16,
            typ: u16,
            clas: u16,
            ttl: u32,
            length: u16,
            addr: interface::V4Addr,
        }

        let cage = interface::cagetable_getref(1);

        let dnssocket = cage.socket_syscall(AF_INET, SOCK_DGRAM, 0);
        assert!(dnssocket > 0);

        let dnsh = DnsHeader {
            xid: 0x1234u16.to_be(),
            flags: 0x0100u16.to_be(),
            qdcount: 0x0001u16.to_be(),
            ancount: 0,
            nscount: 0,
            arcount: 0,
        };

        //specify payload information for dns request
        let hostname = "\x0Bengineering\x03nyu\x03edu\0".to_string().into_bytes(); //numbers signify how many characters until next dot
        let dnstype = 1u16;
        let dnsclass = 1u16;

        //construct packet
        let packetlen = std::mem::size_of::<DnsHeader>()
            + hostname.len()
            + std::mem::size_of::<u16>()
            + std::mem::size_of::<u16>();
        let mut packet = vec![0u8; packetlen];

        let packslice = packet.as_mut_slice();
        let mut pslen = std::mem::size_of::<DnsHeader>();
        unsafe {
            let dnss = ::std::slice::from_raw_parts(
                ((&dnsh) as *const DnsHeader) as *const u8,
                std::mem::size_of::<DnsHeader>(),
            );
            packslice[..pslen].copy_from_slice(dnss);
        }
        packslice[pslen..pslen + hostname.len()].copy_from_slice(hostname.as_slice());
        pslen += hostname.len();
        packslice[pslen..pslen + 2].copy_from_slice(&dnstype.to_be_bytes());
        packslice[pslen + 2..pslen + 4].copy_from_slice(&dnsclass.to_be_bytes());

        //send packet
        let mut dnsaddr = interface::GenSockaddr::V4(interface::SockaddrV4 {
            sin_family: AF_INET as u16,
            // static port is used beacuse this test doesn't bind.
            sin_port: 53u16.to_be(),
            sin_addr: interface::V4Addr {
                s_addr: u32::from_ne_bytes([208, 67, 222, 222]),
            },
            padding: 0,
        }); //opendns ip addr
        assert_eq!(
            cage.sendto_syscall(dnssocket, packslice.as_ptr(), packslice.len(), 0, &dnsaddr),
            packslice.len() as i32
        );

        let mut dnsresp = [0u8; 512];

        //recieve DNS response
        loop {
            let result = cage.recvfrom_syscall(
                dnssocket,
                dnsresp.as_mut_ptr(),
                512,
                0,
                &mut Some(&mut dnsaddr),
            );

            if result != -libc::EINTR {
                assert!(result >= 0);
                break;
            }
            // if the error was EINTR, retry the syscall
        }

        //extract packet header
        let response_header = unsafe { &*(dnsresp.as_ptr() as *const DnsHeader) };
        assert_eq!(u16::from_be(response_header.flags) & 0xf, 0);

        //skip over the name
        let mut nameptr = std::mem::size_of::<DnsHeader>();
        while dnsresp[nameptr] != 0 {
            nameptr += dnsresp[nameptr] as usize + 1;
        }

        //next we need to skip the null byte, qtype, and qclass to extract the main
        // response payload
        let recordptr =
            dnsresp.as_ptr().wrapping_offset(nameptr as isize + 5) as *const DnsRecordAT;
        let record = unsafe { &*recordptr };
        let addr = u32::from_be(record.addr.s_addr);
        assert_eq!(addr, 0x23ac5973); //check that what is returned is the actual ip, 35.172.89.115
                                      //assert_eq!(record.addr.s_addr, 0x7359ac23); //check that what is returned is
                                      // the actual ip, 35.172.89.115

        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_net_domain_socket() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        //bind net zero test reformatted for domain sockets

        let clientsockfilename = "/client.sock";
        let serversockfilename = "/server.sock";

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

        //creating a thread for the server so that the information can be sent between
        // the two threads
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

        //send the data with delays so that the server can process the information
        // cleanly
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

    #[test]
    pub fn ut_lind_net_epoll_create_bad_input() {
        // this test is used for testing epoll_create_syscall with error/edge cases
        // specifically
        // following tests are performed:
        // 1. test for errno with invalid size argument

        // acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        // test for invalid size argument
        assert_eq!(cage.epoll_create_syscall(0), -(Errno::EINVAL as i32));

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_net_epoll_ctl_bad_input() {
        // this test is used for testing epoll_ctl_syscall with error/edge cases
        // specifically
        // following tests are performed:
        // 1. test for errno with invalid fd number
        // 2. test for errno with invalid epfd number
        // 3. test for errno with out of range fd number
        // 4. test for errno with out of range epfd number
        // 5. test for errno when epfd is not epoll instance
        // 6. test for errno when epfd and fd are the same
        // 7. test for errno when fd is a file fd
        // 8. test for errno when trying to modify a fd that does not added to set
        // 9. test for errno when trying to delete a fd that does not added to set
        // 10. test for errno when trying to add a fd that already added to the set
        // 11. test for errno when passing invalid flag

        // acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        // create an epoll instance
        let epfd = cage.epoll_create_syscall(1);

        // create a pipe fd
        let mut pipefds = PipeArray {
            readfd: -1,
            writefd: -1,
        };
        assert_eq!(cage.pipe2_syscall(&mut pipefds, O_NONBLOCK), 0);

        // create a file fd
        let filefd = cage.open_syscall("/netepolltest.txt", O_CREAT | O_EXCL | O_RDWR, S_IRWXA);
        assert!(filefd > 0);

        // test for unexist fd number
        assert_eq!(
            cage.epoll_ctl_syscall(
                epfd,
                EPOLL_CTL_MOD,
                10,
                &mut EpollEvent {
                    events: EPOLLIN as u32,
                    fd: 10,
                }
            ),
            -(Errno::EBADF as i32)
        );

        assert_eq!(
            cage.epoll_ctl_syscall(
                10,
                EPOLL_CTL_MOD,
                pipefds.readfd,
                &mut EpollEvent {
                    events: EPOLLIN as u32,
                    fd: pipefds.readfd,
                }
            ),
            -(Errno::EBADF as i32)
        );

        // test for out of range fd number
        assert_eq!(
            cage.epoll_ctl_syscall(
                epfd,
                EPOLL_CTL_MOD,
                -1,
                &mut EpollEvent {
                    events: EPOLLIN as u32,
                    fd: -1,
                }
            ),
            -(Errno::EBADF as i32)
        );
        assert_eq!(
            cage.epoll_ctl_syscall(
                -1,
                EPOLL_CTL_MOD,
                pipefds.readfd,
                &mut EpollEvent {
                    events: EPOLLIN as u32,
                    fd: pipefds.readfd,
                }
            ),
            -(Errno::EBADF as i32)
        );

        // test for fd that is not epoll fd
        assert_eq!(
            cage.epoll_ctl_syscall(
                filefd,
                EPOLL_CTL_ADD,
                pipefds.readfd,
                &mut EpollEvent {
                    events: EPOLLIN as u32,
                    fd: pipefds.readfd,
                }
            ),
            -(Errno::EINVAL as i32)
        );

        // test when fd and epfd are the same
        assert_eq!(
            cage.epoll_ctl_syscall(
                epfd,
                EPOLL_CTL_ADD,
                epfd,
                &mut EpollEvent {
                    events: EPOLLIN as u32,
                    fd: epfd,
                }
            ),
            -(Errno::EINVAL as i32)
        );

        // test when fd is a file fd
        assert_eq!(
            cage.epoll_ctl_syscall(
                epfd,
                EPOLL_CTL_ADD,
                filefd,
                &mut EpollEvent {
                    events: EPOLLIN as u32,
                    fd: filefd,
                }
            ),
            -(Errno::EPERM as i32)
        );

        // test for modifying fd that does not exists
        assert_eq!(
            cage.epoll_ctl_syscall(
                epfd,
                EPOLL_CTL_MOD,
                pipefds.readfd,
                &mut EpollEvent {
                    events: EPOLLIN as u32,
                    fd: pipefds.readfd,
                }
            ),
            -(Errno::ENOENT as i32)
        );

        // test for deleting fd that does not exists
        assert_eq!(
            cage.epoll_ctl_syscall(
                epfd,
                EPOLL_CTL_DEL,
                pipefds.readfd,
                &mut EpollEvent {
                    events: EPOLLIN as u32,
                    fd: pipefds.readfd,
                }
            ),
            -(Errno::ENOENT as i32)
        );

        // now add a fd
        assert_eq!(
            cage.epoll_ctl_syscall(
                epfd,
                EPOLL_CTL_ADD,
                pipefds.readfd,
                &mut EpollEvent {
                    events: EPOLLIN as u32,
                    fd: pipefds.readfd,
                }
            ),
            0
        );

        // test for adding fd that already exists
        assert_eq!(
            cage.epoll_ctl_syscall(
                epfd,
                EPOLL_CTL_ADD,
                pipefds.readfd,
                &mut EpollEvent {
                    events: EPOLLIN as u32,
                    fd: pipefds.readfd,
                }
            ),
            -(Errno::EEXIST as i32)
        );

        // test for passing invalid flag
        assert_eq!(
            cage.epoll_ctl_syscall(
                epfd,
                123,
                pipefds.readfd,
                &mut EpollEvent {
                    events: EPOLLIN as u32,
                    fd: pipefds.readfd,
                }
            ),
            -(Errno::EINVAL as i32)
        );

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_net_epoll_wait_bad_input() {
        // this test is used for testing epoll_wait_syscall with error/edge cases
        // specifically
        // following tests are performed:
        // 1. test for errno with out of range fd number
        // 2. test for errno with invalid fd number
        // 3. test for errno when fd is not an epoll instance
        // 4. test for errno with invalid maxevents argument

        // acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        // create an epoll instance
        let epfd = cage.epoll_create_syscall(1);

        // create a pipe fd
        let mut pipefds = PipeArray {
            readfd: -1,
            writefd: -1,
        };
        assert_eq!(cage.pipe2_syscall(&mut pipefds, O_NONBLOCK), 0);

        // create a file fd
        let filefd = cage.open_syscall("/netepolltest.txt", O_CREAT | O_EXCL | O_RDWR, S_IRWXA);
        assert!(filefd > 0);

        let mut event_list: Vec<EpollEvent> = vec![
            EpollEvent {
                events: EPOLLIN as u32,
                fd: 0,
            };
            2
        ];

        // test for out of range fd range
        assert_eq!(
            cage.epoll_wait_syscall(-1, &mut event_list, 1, None),
            -(Errno::EBADF as i32)
        );

        // test for invalid fd range
        assert_eq!(
            cage.epoll_wait_syscall(10, &mut event_list, 1, None),
            -(Errno::EBADF as i32)
        );

        // test for fd that is not epoll
        assert_eq!(
            cage.epoll_wait_syscall(filefd, &mut event_list, 1, None),
            -(Errno::EINVAL as i32)
        );

        // test for invalid maxevents argument
        assert_eq!(
            cage.epoll_wait_syscall(epfd, &mut event_list, 0, None),
            -(Errno::EINVAL as i32)
        );

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_net_epoll_maxevents_arg() {
        // this test is used for testing maxevents argument of epoll_wait_syscall
        // specifically

        // acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        let pipe_num = 10;
        // create some pipes
        let mut pipefds = vec![
            PipeArray {
                readfd: -1,
                writefd: -1,
            };
            pipe_num
        ];
        for pipefd in pipefds.iter_mut() {
            assert_eq!(cage.pipe2_syscall(pipefd, O_NONBLOCK), 0);
        }

        // create an epoll instance
        let epfd = cage.epoll_create_syscall(1);
        // add all pipes to epoll
        for pipefd in pipefds.iter_mut() {
            assert_eq!(
                cage.epoll_ctl_syscall(
                    epfd,
                    EPOLL_CTL_ADD,
                    pipefd.readfd,
                    &mut EpollEvent {
                        events: EPOLLIN as u32,
                        fd: pipefd.readfd,
                    }
                ),
                0
            );

            // write something to the pipe at the same time
            assert_eq!(cage.write_syscall(pipefd.writefd, str2cbuf("test"), 4), 4);
        }

        // at this point, all pipes are added to epoll, and they should all be readable

        // prepare the event_list to store the return value
        let mut event_list: Vec<EpollEvent> = vec![EpollEvent { events: 0, fd: 0 }; pipe_num];

        // test #1: all the fds should be ready
        assert_eq!(
            cage.epoll_wait_syscall(epfd, &mut event_list, pipe_num as i32, None),
            pipe_num as i32
        );
        for event in event_list.iter() {
            // check if all fd are marked as readable
            assert_ne!(event.events & (EPOLLIN as u32), 0);
        }

        // test #2: maxevents set to be smaller than pipe_num

        // clear event_list
        let mut event_list: Vec<EpollEvent> = vec![
            EpollEvent {
                events: 0xdeadbeef,
                fd: 0,
            };
            pipe_num
        ];

        assert_eq!(cage.epoll_wait_syscall(epfd, &mut event_list, 5, None), 5);

        for i in 0..pipe_num {
            // even though all fds are ready, only first 5 should be marked with EPOLLIN
            if i < 5 {
                assert_ne!(event_list[i].events & (EPOLLIN as u32), 0);
            } else {
                assert_eq!(event_list[i].events, 0xdeadbeef);
            }
        }

        // test #3: maxevents set to be larger than actual ready fds (case 1)

        // clear event_list
        let mut event_list: Vec<EpollEvent> = vec![
            EpollEvent {
                events: 0xdeadbeef,
                fd: 0,
            };
            pipe_num
        ];
        // first let's consume some pipes
        let mut buf = sizecbuf(4);
        assert_eq!(cage.read_syscall(pipefds[0].readfd, buf.as_mut_ptr(), 4), 4);
        assert_eq!(cbuf2str(&buf), "test");
        assert_eq!(cage.read_syscall(pipefds[1].readfd, buf.as_mut_ptr(), 4), 4);
        assert_eq!(cbuf2str(&buf), "test");
        // now pipe with index 0 and 1 are consumed and they are no longer readable
        assert_eq!(
            cage.epoll_wait_syscall(epfd, &mut event_list, pipe_num as i32, None),
            (pipe_num - 2) as i32
        );

        for i in 0..pipe_num {
            // even though all fds are ready, only first 5 should be marked with EPOLLIN
            if i < 8 {
                assert_ne!(event_list[i].events & (EPOLLIN as u32), 0);
            } else {
                assert_eq!(event_list[i].events, 0xdeadbeef);
            }
        }

        // test #4: maxevents set to be larger than actual ready fds (case 2)
        // clear event_list
        let mut event_list: Vec<EpollEvent> = vec![
            EpollEvent {
                events: 0xdeadbeef,
                fd: 0,
            };
            pipe_num
        ];
        // we try to only read 5 this time
        // since the number of avaliable fds is still 8
        // so it is supposed to return 5
        assert_eq!(cage.epoll_wait_syscall(epfd, &mut event_list, 5, None), 5);
        for i in 0..pipe_num {
            // only first 5 should be marked with EPOLLIN and others remain untouched
            if i < 5 {
                assert_ne!(event_list[i].events & (EPOLLIN as u32), 0);
            } else {
                assert_eq!(event_list[i].events, 0xdeadbeef);
            }
        }

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_net_epoll_timeout() {
        // this test is used for testing timeout argument of epoll_wait_syscall
        // specifically
        // following tests are performed:
        // 1. test for epoll_wait when timeout could expire
        // 2. test for epoll_wait when not fd is monitored but timeout is set

        // acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();
        let cage = interface::cagetable_getref(1);

        // subtest 1: epoll when timeout could expire
        // create a TCP AF_UNIX socket
        let serversockfd = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        let clientsockfd = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        assert!(serversockfd > 0);
        assert!(clientsockfd > 0);

        let port: u16 = generate_random_port();
        let sockaddr = interface::SockaddrV4 {
            sin_family: AF_INET as u16,
            sin_port: port.to_be(),
            sin_addr: interface::V4Addr {
                s_addr: u32::from_ne_bytes([127, 0, 0, 1]),
            },
            padding: 0,
        };
        let socket = interface::GenSockaddr::V4(sockaddr); //127.0.0.1 from bytes above

        // server bind and listen
        assert_eq!(cage.bind_syscall(serversockfd, &socket), 0);
        assert_eq!(cage.listen_syscall(serversockfd, 4), 0);

        assert_eq!(cage.fork_syscall(2), 0);
        assert_eq!(cage.close_syscall(clientsockfd), 0);

        // this barrier is used for preventing
        // an unfixed bug (`close` could block when other thread/cage is `accept`) from
        // deadlocking the test
        let barrier = Arc::new(Barrier::new(2));
        let barrier_2 = barrier.clone();

        // client connects to the server to send and recv data...
        let threadclient = interface::helper_thread(move || {
            let cage2 = interface::cagetable_getref(2);
            assert_eq!(cage2.close_syscall(serversockfd), 0);

            barrier_2.wait();

            // connect to the server
            assert_eq!(cage2.connect_syscall(clientsockfd, &socket), 0);

            // wait for 100ms
            interface::sleep(interface::RustDuration::from_millis(100));

            // send some message to client
            assert_eq!(cage2.send_syscall(clientsockfd, str2cbuf("test"), 4, 0), 4);

            assert_eq!(cage2.close_syscall(clientsockfd), 0);
            cage2.exit_syscall(EXIT_SUCCESS);
        });

        // make sure client thread closed the duplicated socket before server start to
        // accept
        barrier.wait();

        // wait for client to connect
        let mut sockgarbage = interface::GenSockaddr::V4(interface::SockaddrV4::default());
        let sockfd = cage.accept_syscall(serversockfd as i32, &mut sockgarbage);

        // add to client socket to epoll
        // perform another test by the way: the fd field of EpollEvent is user data by
        // standard which means we could put anything we want here, and kernel
        // is not supposed to touch it
        let epfd = cage.epoll_create_syscall(1);
        assert_eq!(
            cage.epoll_ctl_syscall(
                epfd,
                EPOLL_CTL_ADD,
                sockfd,
                &mut EpollEvent {
                    events: EPOLLIN as u32,
                    fd: 123,
                }
            ),
            0
        );
        // event_list used for holding return value
        let mut event_list: Vec<EpollEvent> = vec![EpollEvent { events: 0, fd: 0 }];

        // this counter is used for recording how many times do select returns due to
        // timeout
        let mut counter = 0;

        loop {
            let epoll_result = cage.epoll_wait_syscall(
                epfd,
                &mut event_list,
                1,
                Some(interface::RustDuration::new(0, 10000000)), // 10ms
            );
            assert!(epoll_result >= 0);
            // epoll timeout after 10ms, but client will send messages after 100ms
            // so there should be some timeout return
            if epoll_result == 0 {
                counter += 1;
            } else if event_list[0].events & (EPOLLIN as u32) != 0 {
                assert_eq!(event_list[0].fd, 123); // fd field should remain touched
                                                   // just received the message, check the message and break
                let mut buf = sizecbuf(4);
                assert_eq!(cage.recv_syscall(sockfd, buf.as_mut_ptr(), 4, 0), 4);
                assert_eq!(cbuf2str(&buf), "test");
                break;
            } else {
                unreachable!();
            }
        }
        // check if epoll timeout correctly
        assert!(counter > 0);

        threadclient.join().unwrap();

        // subtest 2: epoll when nothing is monitored, `epoll` here should behave like
        // `sleep`
        let epfd2 = cage.epoll_create_syscall(1);

        let start_time = interface::starttimer();
        let timeout = interface::RustDuration::new(0, 10000000); // 10ms
        let epoll_result = cage.epoll_wait_syscall(epfd2, &mut event_list, 1, Some(timeout));
        assert!(epoll_result == 0);
        // should wait for at least 10ms
        assert!(interface::readtimer(start_time) >= timeout);

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    #[ignore]
    pub fn ut_lind_net_epoll() {
        // test for epoll monitoring on multiple different file descriptors:
        // 1. AF_INET server socket waiting for two clients
        // 2. AF_INET server socket's connection file descriptor with clients
        // 3. AF_UNIX server socket's connection file descriptor with a client
        // 4. pipe

        // acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();
        let cage = interface::cagetable_getref(1);

        // creating socket file descriptors
        let serversockfd = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        let clientsockfd1 = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        let clientsockfd2 = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        let serversockfd_unix = cage.socket_syscall(AF_UNIX, SOCK_STREAM, 0);
        let clientsockfd_unix = cage.socket_syscall(AF_UNIX, SOCK_STREAM, 0);

        assert!(serversockfd > 0);
        assert!(clientsockfd1 > 0);
        assert!(clientsockfd2 > 0);
        assert!(serversockfd_unix > 0);
        assert!(clientsockfd_unix > 0);

        // creating a pipe
        let mut pipefds = PipeArray {
            readfd: -1,
            writefd: -1,
        };
        assert_eq!(cage.pipe_syscall(&mut pipefds), 0);

        // create a INET address
        let port: u16 = generate_random_port();

        let sockaddr = interface::SockaddrV4 {
            sin_family: AF_INET as u16,
            sin_port: port.to_be(),
            sin_addr: interface::V4Addr {
                s_addr: u32::from_ne_bytes([127, 0, 0, 1]),
            },
            padding: 0,
        };
        let socket = interface::GenSockaddr::V4(sockaddr); //127.0.0.1 from bytes above

        //binding to a socket
        let serversockaddr_unix =
            interface::new_sockaddr_unix(AF_UNIX as u16, "server_poll".as_bytes());
        let serversocket_unix = interface::GenSockaddr::Unix(serversockaddr_unix);

        let clientsockaddr_unix =
            interface::new_sockaddr_unix(AF_UNIX as u16, "client_poll".as_bytes());
        let clientsocket_unix = interface::GenSockaddr::Unix(clientsockaddr_unix);

        assert_eq!(cage.bind_syscall(serversockfd_unix, &serversocket_unix), 0);
        assert_eq!(cage.bind_syscall(clientsockfd_unix, &clientsocket_unix), 0);
        assert_eq!(cage.listen_syscall(serversockfd_unix, 1), 0);

        assert_eq!(cage.bind_syscall(serversockfd, &socket), 0);
        assert_eq!(cage.listen_syscall(serversockfd, 4), 0);

        // create epoll file descriptor
        let epfd = cage.epoll_create_syscall(1);
        assert!(epfd > 0);

        // add file descriptors to epoll
        cage.epoll_ctl_syscall(
            epfd,
            EPOLL_CTL_ADD,
            serversockfd,
            &mut EpollEvent {
                events: EPOLLIN as u32,
                fd: serversockfd,
            },
        );

        cage.epoll_ctl_syscall(
            epfd,
            EPOLL_CTL_ADD,
            serversockfd_unix,
            &mut EpollEvent {
                events: EPOLLIN as u32,
                fd: serversockfd_unix,
            },
        );

        cage.epoll_ctl_syscall(
            epfd,
            EPOLL_CTL_ADD,
            pipefds.readfd,
            &mut EpollEvent {
                events: EPOLLIN as u32,
                fd: pipefds.readfd,
            },
        );

        assert_eq!(cage.fork_syscall(2), 0); // used for AF_INET thread client 1
        assert_eq!(cage.fork_syscall(3), 0); // used for AF_INET thread client 2
        assert_eq!(cage.fork_syscall(4), 0); // used for AF_UNIX thread client

        assert_eq!(cage.fork_syscall(5), 0); // used for pipe thread

        assert_eq!(cage.close_syscall(clientsockfd1), 0);
        assert_eq!(cage.close_syscall(clientsockfd2), 0);
        assert_eq!(cage.close_syscall(clientsockfd_unix), 0);

        // this barrier have to ensure that the clients finish the connect before we do
        // the epoll due to an unfixed bug (`close` could block when other
        // thread/cage is `accept`)
        let barrier = Arc::new(Barrier::new(3));
        let barrier_clone1 = barrier.clone();
        let barrier_clone2 = barrier.clone();

        // this barrier is used for control the flow the pipe
        let barrier_pipe = Arc::new(Barrier::new(2));
        let barrier_pipe_clone = barrier_pipe.clone();

        // due to an unfixed bug in ref counter of AF_UNIX socket pipe
        // have to make sure all the threads exits only after the AF_UNIX test finished
        let barrier_exit = Arc::new(Barrier::new(4));
        let barrier_exit_clone1 = barrier_exit.clone();
        let barrier_exit_clone2 = barrier_exit.clone();
        let barrier_exit_clone3 = barrier_exit.clone();

        // client 1 connects to the server to send and recv data
        let threadclient1 = interface::helper_thread(move || {
            let cage2 = interface::cagetable_getref(2);
            assert_eq!(cage2.close_syscall(serversockfd), 0);
            assert_eq!(cage2.close_syscall(clientsockfd2), 0);

            // connect to server
            assert_eq!(cage2.connect_syscall(clientsockfd1, &socket), 0);
            barrier_clone1.wait();

            // send message to server
            assert_eq!(cage2.send_syscall(clientsockfd1, str2cbuf("test"), 4, 0), 4);

            interface::sleep(interface::RustDuration::from_millis(1));

            // receive message from server
            let mut buf = sizecbuf(4);
            assert_eq!(cage2.recv_syscall(clientsockfd1, buf.as_mut_ptr(), 4, 0), 4);
            assert_eq!(cbuf2str(&buf), "test");

            assert_eq!(cage2.close_syscall(clientsockfd1), 0);
            barrier_exit_clone1.wait();
            cage2.exit_syscall(EXIT_SUCCESS);
        });

        // client 2 connects to the server to send and recv data
        let threadclient2 = interface::helper_thread(move || {
            let cage3 = interface::cagetable_getref(3);
            assert_eq!(cage3.close_syscall(serversockfd), 0);
            assert_eq!(cage3.close_syscall(clientsockfd1), 0);

            // connect to server
            assert_eq!(cage3.connect_syscall(clientsockfd2, &socket), 0);
            barrier_clone2.wait();

            // send message to server
            assert_eq!(cage3.send_syscall(clientsockfd2, str2cbuf("test"), 4, 0), 4);

            interface::sleep(interface::RustDuration::from_millis(1));

            // receive message from server
            let mut buf = sizecbuf(4);
            let mut result: i32;
            loop {
                result = cage3.recv_syscall(clientsockfd2, buf.as_mut_ptr(), 4, 0);
                if result != -libc::EINTR {
                    break; // if the error was EINTR, retry the syscall
                }
            }
            assert_eq!(result, 4);
            assert_eq!(cbuf2str(&buf), "test");

            assert_eq!(cage3.close_syscall(clientsockfd2), 0);
            barrier_exit_clone2.wait();
            cage3.exit_syscall(EXIT_SUCCESS);
        });

        let threadclient_unix = interface::helper_thread(move || {
            let cage4 = interface::cagetable_getref(4);
            assert_eq!(cage4.close_syscall(serversockfd_unix), 0);
            assert_eq!(cage4.close_syscall(serversockfd), 0);

            // connect to server
            assert_eq!(
                cage4.connect_syscall(clientsockfd_unix, &serversocket_unix),
                0
            );

            // send message to server
            assert_eq!(
                cage4.send_syscall(clientsockfd_unix, str2cbuf("test"), 4, 0),
                4
            );

            interface::sleep(interface::RustDuration::from_millis(1));

            // recieve message from server
            let mut buf = sizecbuf(4);
            let mut result: i32;
            loop {
                result = cage4.recv_syscall(clientsockfd_unix, buf.as_mut_ptr(), 4, 0);
                if result != -libc::EINTR {
                    break; // if the error was EINTR, retry the syscall
                }
            }
            assert_eq!(result, 4);
            assert_eq!(cbuf2str(&buf), "test");

            assert_eq!(cage4.close_syscall(clientsockfd_unix), 0);
            cage4.exit_syscall(EXIT_SUCCESS);
        });

        let thread_pipe = interface::helper_thread(move || {
            let cage5 = interface::cagetable_getref(5);

            interface::sleep(interface::RustDuration::from_millis(1));
            // send message to pipe
            assert_eq!(cage5.write_syscall(pipefds.writefd, str2cbuf("test"), 4), 4);

            let mut buf = sizecbuf(5);
            // wait until peer read the message
            barrier_pipe_clone.wait();

            // read the message sent by peer
            assert_eq!(cage5.read_syscall(pipefds.readfd, buf.as_mut_ptr(), 5), 5);
            assert_eq!(cbuf2str(&buf), "test2");

            barrier_exit_clone3.wait();
            cage5.exit_syscall(EXIT_SUCCESS);
        });

        barrier.wait();

        let event_list_size = 5;
        let mut event_list: Vec<EpollEvent> =
            vec![EpollEvent { events: 0, fd: 0 }; event_list_size];
        // acting as the server and processing the request
        // Server loop to handle connections and I/O
        // Check for any activity in any of the Input sockets
        for _counter in 0..600 {
            // epoll call
            let num_events = cage.epoll_wait_syscall(
                epfd,
                &mut event_list,
                event_list_size as i32,
                Some(interface::RustDuration::ZERO),
            );
            assert!(num_events >= 0); // check for error

            for event in &mut event_list[..num_events as usize] {
                // Check for any activity in the input socket and if there are events ready for
                // reading
                if event.events & (EPOLLIN as u32) != 0 {
                    // If the socket returned was listener socket, then there's a new connection
                    if event.fd == serversockfd {
                        let mut sockgarbage =
                            interface::GenSockaddr::V4(interface::SockaddrV4::default());
                        let sockfd = cage.accept_syscall(event.fd as i32, &mut sockgarbage);
                        assert!(sockfd > 0);
                        let event = interface::EpollEvent {
                            events: EPOLLIN as u32 | EPOLLOUT as u32,
                            fd: sockfd,
                        };
                        // Error raised to indicate that the socket file descriptor couldn't be
                        // added to the epoll instance
                        assert_eq!(
                            cage.epoll_ctl_syscall(epfd, EPOLL_CTL_ADD, sockfd, &event),
                            0
                        );
                    } else if event.fd == serversockfd_unix {
                        // unix socket
                        let mut sockgarbage = interface::GenSockaddr::Unix(
                            interface::new_sockaddr_unix(AF_UNIX as u16, "".as_bytes()),
                        );
                        let sockfd = cage.accept_syscall(event.fd as i32, &mut sockgarbage);
                        assert!(sockfd > 0);
                        let event = interface::EpollEvent {
                            events: EPOLLIN as u32 | EPOLLOUT as u32,
                            fd: sockfd,
                        };
                        // Error raised to indicate that the socket file descriptor couldn't be
                        // added to the epoll instance
                        assert_eq!(
                            cage.epoll_ctl_syscall(epfd, EPOLL_CTL_ADD, sockfd, &event),
                            0
                        );
                    } else if event.fd == pipefds.readfd {
                        // pipe
                        let mut buf = sizecbuf(4);
                        // read the message from peer
                        assert_eq!(cage.read_syscall(pipefds.readfd, buf.as_mut_ptr(), 4), 4);
                        assert_eq!(cbuf2str(&buf), "test");

                        // write the message from peer
                        assert_eq!(
                            cage.write_syscall(pipefds.writefd, str2cbuf("test2"), 5) as usize,
                            5
                        );
                        barrier_pipe.wait();

                        // pipe epoll test done
                        assert_eq!(
                            cage.epoll_ctl_syscall(
                                epfd,
                                EPOLL_CTL_DEL,
                                event.fd,
                                &EpollEvent { events: 0, fd: 0 }
                            ),
                            0
                        );
                    } else {
                        //If the socket is in established conn., then we recv the data. If there's
                        // no data, then close the client socket.
                        let mut buf = sizecbuf(4);
                        let mut recvresult: i32;
                        loop {
                            // receive message from peer
                            recvresult = cage.recv_syscall(event.fd as i32, buf.as_mut_ptr(), 4, 0);
                            if recvresult != -libc::EINTR {
                                break; // if the error was EINTR, retry the
                                       // syscall
                            }
                        }
                        if recvresult == 4 {
                            if cbuf2str(&buf) == "test" {
                                continue;
                            }
                        } else if recvresult == -libc::ECONNRESET {
                            // peer closed the connection
                            assert_eq!(cage.close_syscall(event.fd as i32), 0);
                            assert_eq!(
                                cage.epoll_ctl_syscall(
                                    epfd,
                                    EPOLL_CTL_DEL,
                                    event.fd,
                                    &EpollEvent { events: 0, fd: 0 }
                                ),
                                0
                            );
                        }
                    }
                }
                if event.events & (EPOLLOUT as u32) != 0 {
                    // Data is sent out this socket, it's no longer ready for writing
                    assert_eq!(
                        cage.send_syscall(event.fd as i32, str2cbuf("test"), 4, 0),
                        4
                    );
                    // remove the fd
                    assert_eq!(
                        cage.epoll_ctl_syscall(
                            epfd,
                            EPOLL_CTL_DEL,
                            event.fd,
                            &EpollEvent { events: 0, fd: 0 }
                        ),
                        0
                    );
                }
            }
        }
        assert_eq!(cage.close_syscall(serversockfd), 0);
        assert_eq!(cage.close_syscall(serversockfd_unix), 0);

        // let threads exit
        barrier_exit.wait();

        threadclient1.join().unwrap();
        threadclient2.join().unwrap();
        threadclient_unix.join().unwrap();
        thread_pipe.join().unwrap();

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_net_writev() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();

        let cage = interface::cagetable_getref(1);

        let serversockfd = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        let clientsockfd = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);

        //making sure that the assigned fd's are valid
        assert!(serversockfd > 0);
        assert!(clientsockfd > 0);
        let port: u16 = generate_random_port();

        //binding to a socket
        let sockaddr = interface::SockaddrV4 {
            sin_family: AF_INET as u16,
            sin_port: port.to_be(),
            sin_addr: interface::V4Addr {
                s_addr: u32::from_ne_bytes([127, 0, 0, 1]),
            },
            padding: 0,
        };
        let socket = interface::GenSockaddr::V4(sockaddr); //127.0.0.1
        assert_eq!(cage.bind_syscall(serversockfd, &socket), 0);
        assert_eq!(cage.listen_syscall(serversockfd, 1), 0); //we are only allowing for one client at a time

        //forking the cage to get another cage with the same information
        assert_eq!(cage.fork_syscall(2), 0);

        //creating a thread for the server so that the information can be sent between
        // the two threads
        let thread = interface::helper_thread(move || {
            let cage2 = interface::cagetable_getref(2);
            interface::sleep(interface::RustDuration::from_millis(100));
            let port: u16 = generate_random_port();

            let mut socket2 = interface::GenSockaddr::V4(interface::SockaddrV4 {
                sin_family: AF_INET as u16,
                sin_port: port.to_be(),
                sin_addr: interface::V4Addr {
                    s_addr: u32::from_ne_bytes([127, 0, 0, 1]),
                },
                padding: 0,
            }); //127.0.0.1
            let sockfd = cage2.accept_syscall(serversockfd, &mut socket2); //really can only make sure that the fd is valid
            assert!(sockfd > 0);

            //process the first test...
            //Writing 100, then peek 100, then read 100
            let mut buf = sizecbuf(300);
            assert_eq!(
                cage2.recvfrom_syscall(sockfd, buf.as_mut_ptr(), 300, 0, &mut Some(&mut socket2)),
                300
            ); //reading the input message

            assert_eq!(cage2.close_syscall(sockfd), 0);
            assert_eq!(cage2.close_syscall(serversockfd), 0);
            assert_eq!(cage2.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        });

        //connect to the server
        assert_eq!(cage.connect_syscall(clientsockfd, &socket), 0);

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

        assert_eq!(cage.writev_syscall(clientsockfd, iovec.as_ptr(), 3), 300);

        thread.join().unwrap();

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }
}
