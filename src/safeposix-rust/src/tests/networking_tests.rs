#[cfg(test)]
pub mod net_tests {
    use super::super::*;
    use crate::interface;
    use crate::safeposix::{cage::*, dispatcher::*, filesystem};
    use libc::c_void;
    use std::mem::size_of;
    use std::sync::{Arc, Barrier};

    pub fn net_tests() {
        ut_lind_net_bind();
        ut_lind_net_bind_multiple();
        ut_lind_net_bind_on_zero();
        ut_lind_net_connect_basic_udp();
        ut_lind_net_getpeername();
        ut_lind_net_getsockname();
        ut_lind_net_listen();
        ut_lind_net_poll();
        ut_lind_net_recvfrom();
        ut_lind_net_select();
        ut_lind_net_shutdown();
        ut_lind_net_socket();
        ut_lind_net_socketoptions();
        ut_lind_net_socketpair();
        ut_lind_net_udp_bad_bind();
        ut_lind_net_udp_simple();
        ut_lind_net_udp_connect();
        ut_lind_net_gethostname();
        ut_lind_net_dns_rootserver_ping();
        ut_lind_net_domain_socket();
        ut_lind_net_epoll();
        ut_lind_net_writev();
    }

    pub fn ut_lind_net_bind() {
        lindrustinit(0);
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

    pub fn ut_lind_net_bind_on_zero() {
        lindrustinit(0);
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

        //creating a thread for the server so that the information can be sent between the two threads
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

        //connect to the server with the other sockfd
        assert_eq!(cage.connect_syscall(clientsockfd2, &socket), 0);

        //send the data with delays so that the server can process the information cleanly
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

    pub fn ut_lind_net_bind_multiple() {
        lindrustinit(0);
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

    pub fn ut_lind_net_connect_basic_udp() {
        lindrustinit(0);
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

    pub fn ut_lind_net_getpeername() {
        lindrustinit(0);
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

    pub fn ut_lind_net_getsockname() {
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);

        let sockfd = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        let mut retsocket = interface::GenSockaddr::V4(interface::SockaddrV4::default());

        assert_eq!(cage.getsockname_syscall(sockfd, &mut retsocket), 0);
        assert_eq!(retsocket.port(), 0);
        assert_eq!(
            retsocket.addr(),
            interface::GenIpaddr::V4(interface::V4Addr::default())
        );
        let port: u16 = generate_random_port();
        let socket = interface::GenSockaddr::V4(interface::SockaddrV4 {
            sin_family: AF_INET as u16,
            sin_port: port.to_be(),
            sin_addr: interface::V4Addr {
                s_addr: u32::from_ne_bytes([127, 0, 0, 1]),
            },
            padding: 0,
        }); //127.0.0.1

        assert_eq!(cage.bind_syscall(sockfd, &socket), 0);
        assert_eq!(cage.getsockname_syscall(sockfd, &mut retsocket), 0);
        assert_eq!(retsocket, socket);

        //checking that we cannot rebind the socket
        assert_eq!(cage.bind_syscall(sockfd, &socket), -(Errno::EINVAL as i32)); //already bound so should fail
        assert_eq!(cage.getsockname_syscall(sockfd, &mut retsocket), 0);
        assert_eq!(retsocket, socket);

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_net_listen() {
        lindrustinit(0);
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

    pub fn ut_lind_net_poll() {
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);

        let filefd = cage.open_syscall("/netpolltest.txt", O_CREAT | O_EXCL | O_RDWR, S_IRWXA);
        assert!(filefd > 0);

        let serversockfd = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        let clientsockfd1 = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        let clientsockfd2 = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
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
        assert_eq!(cage.bind_syscall(serversockfd, &socket), 0);
        assert_eq!(cage.listen_syscall(serversockfd, 4), 0);

        let serverpoll = interface::PollStruct {
            fd: serversockfd,
            events: POLLIN,
            revents: 0,
        };
        let filepoll = interface::PollStruct {
            fd: filefd,
            events: POLLIN,
            revents: 0,
        };
        let mut polled = vec![serverpoll, filepoll];

        cage.fork_syscall(2);
        //client 1 connects to the server to send and recv data...
        let thread1 = interface::helper_thread(move || {
            interface::sleep(interface::RustDuration::from_millis(30));
            let cage2 = interface::cagetable_getref(2);

            assert_eq!(cage2.connect_syscall(clientsockfd1, &socket), 0);
            assert_eq!(
                cage2.send_syscall(clientsockfd1, str2cbuf(&"test"), 4, 0),
                4
            );
            //giving it a longer pause time to that it can process all of the data that it is recieving
            interface::sleep(interface::RustDuration::from_millis(100));

            assert_eq!(cage2.close_syscall(serversockfd), 0);
            cage2.exit_syscall(EXIT_SUCCESS);
        });

        cage.fork_syscall(3);
        //client 2 connects to the server to send and recv data...
        let thread2 = interface::helper_thread(move || {
            //give it a longer time so that it can sufficiently process all of the data
            interface::sleep(interface::RustDuration::from_millis(45));
            let cage3 = interface::cagetable_getref(3);

            assert_eq!(cage3.connect_syscall(clientsockfd2, &socket), 0);
            assert_eq!(
                cage3.send_syscall(clientsockfd2, str2cbuf(&"test"), 4, 0),
                4
            );

            interface::sleep(interface::RustDuration::from_millis(100));

            assert_eq!(cage3.close_syscall(serversockfd), 0);
            cage3.exit_syscall(EXIT_SUCCESS);
        });

        //acting as the server and processing the request
        let thread3 = interface::helper_thread(move || {
            let mut infds: Vec<i32>;
            let mut outfds: Vec<i32>;
            for _counter in 0..600 {
                //start a while true loop for processing requests
                let pollretvalue = cage.poll_syscall(
                    &mut polled.as_mut_slice(),
                    Some(interface::RustDuration::ZERO),
                );
                assert!(pollretvalue >= 0);

                infds = vec![];
                outfds = vec![];

                for polledfile in &mut polled {
                    if polledfile.revents & POLLIN != 0 {
                        infds.push(polledfile.fd);
                    }
                    if polledfile.revents & POLLOUT != 0 {
                        outfds.push(polledfile.fd);
                    }
                }

                //check for any activity in the input sockets
                for sockfd in infds {
                    //If the socket returned was listerner socket, then there's a new connection
                    //so we accept it, and put the client socket in the list of inputs.
                    if sockfd == serversockfd {
                        let port: u16 = generate_random_port();
                        let sockaddr = interface::SockaddrV4 {
                            sin_family: AF_INET as u16,
                            sin_port: port.to_be(),
                            sin_addr: interface::V4Addr {
                                s_addr: u32::from_ne_bytes([127, 0, 0, 1]),
                            },
                            padding: 0,
                        };
                        let mut addr = interface::GenSockaddr::V4(sockaddr); //127.0.0.1 from bytes above

                        let newsockfd = cage.accept_syscall(sockfd, &mut addr);
                        polled.push(interface::PollStruct {
                            fd: newsockfd,
                            events: POLLIN,
                            revents: 0,
                        })
                    } else if sockfd == filefd {
                        //Write to a file...
                        assert_eq!(cage.write_syscall(sockfd, str2cbuf("test"), 4), 4);
                        assert_eq!(cage.lseek_syscall(sockfd, 0, SEEK_SET), 0);
                        //Once the write is successful into a file, modify the file descriptor so that its ready for reading out of the file.
                        for polledfile in &mut polled {
                            if polledfile.fd == sockfd {
                                polledfile.events = POLLOUT;
                                break;
                            }
                        }
                    } else {
                        //If the socket is in established conn., then we recv the data. If there's no data, then close the client socket.
                        let mut buf = sizecbuf(4);
                        let mut result: i32;
                        loop {
                            result = cage.recv_syscall(sockfd, buf.as_mut_ptr(), 4, 0);
                            if result != -libc::EINTR {
                                assert_eq!(result & !4, 0); //This must be 0 or 4 to be correct, either the socket is good for recieving or it's closed
                                break; // if the error was EINTR, retry the syscall
                            }
                        }
                        if result == 4 {
                            assert_eq!(cbuf2str(&buf), "test");
                            //This socket is ready for writing, modify the socket descriptor to be in read-write mode. This socket can write data out to network
                            for polledfile in &mut polled {
                                if polledfile.fd == sockfd {
                                    polledfile.events = POLLOUT;
                                    break;
                                }
                            }
                        } else {
                            //No data means remote socket closed, hence close the client socket in server, also remove this socket from polling.
                            assert_eq!(cage.close_syscall(sockfd), 0);
                            polled.retain(|x| x.fd != sockfd);
                        }
                    }
                }

                for sockfd in outfds {
                    if sockfd == filefd {
                        let mut read_buf1 = sizecbuf(4);
                        assert_eq!(cage.read_syscall(sockfd, read_buf1.as_mut_ptr(), 4), 4);
                        assert_eq!(cbuf2str(&read_buf1), "test");
                        //test for file finished, remove from polling.
                        polled.retain(|x| x.fd != sockfd);
                    } else {
                        //Data is sent out of this socket, it's no longer ready for writing, modify it only read mode.
                        assert_eq!(cage.send_syscall(sockfd, str2cbuf(&"test"), 4, 0), 4);
                        for polledfile in &mut polled {
                            if polledfile.fd == sockfd {
                                polledfile.events = POLLIN;
                            }
                        }
                    }
                }
            }
            assert_eq!(cage.close_syscall(serversockfd), 0);
            assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        });

        thread1.join().unwrap();
        thread2.join().unwrap();
        thread3.join().unwrap();

        lindrustfinalize();
    }

    pub fn ut_lind_net_recvfrom() {
        lindrustinit(0);
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

        //creating a thread for the server so that the information can be sent between the two threads
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

        thread.join().unwrap();

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_net_select() {
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);

        let filefd = cage.open_syscall("/netselecttest.txt", O_CREAT | O_EXCL | O_RDWR, S_IRWXA);
        assert!(filefd > 0);

        let serversockfd = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        let clientsockfd1 = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        let clientsockfd2 = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
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
        assert_eq!(cage.bind_syscall(serversockfd, &socket), 0);
        assert_eq!(cage.listen_syscall(serversockfd, 4), 0);

        // allocate spaces for fd_set bitmaps
        // `master_set`: Consits of all file descriptors.
        // `working_set`: Consits of a copy of `master_set`. Modified by `select()` to contain only ready descriptors. 
        let master_set = &mut interface::FdSet::new();
        let working_set = &mut interface::FdSet::new();
        let outputs = &mut interface::FdSet::new();

        master_set.set(serversockfd);
        master_set.set(filefd);
        outputs.set(filefd);

        assert_eq!(master_set.is_set(serversockfd), true);
        assert_eq!(master_set.is_set(filefd), true);
        assert_eq!(outputs.is_set(filefd), true);

        assert_eq!(cage.fork_syscall(2), 0);
        assert_eq!(cage.fork_syscall(3), 0);

        assert_eq!(cage.close_syscall(clientsockfd1), 0);
        assert_eq!(cage.close_syscall(clientsockfd2), 0);

        // these barriers ensures that the clients finish the connect before we do the select
        let barrier = Arc::new(Barrier::new(3));
        let barrier_clone1 = barrier.clone();
        let barrier_clone2 = barrier.clone();

        //client 1 connects to the server to send and recv data...
        let threadclient1 = interface::helper_thread(move || {
            let cage2 = interface::cagetable_getref(2);
            assert_eq!(cage2.close_syscall(serversockfd), 0);

            assert_eq!(cage2.connect_syscall(clientsockfd1, &socket), 0);
            barrier_clone1.wait();
            assert_eq!(cage2.send_syscall(clientsockfd1, str2cbuf("test"), 4, 0), 4);

            interface::sleep(interface::RustDuration::from_millis(1));

            let mut buf = sizecbuf(4);
            assert_eq!(cage2.recv_syscall(clientsockfd1, buf.as_mut_ptr(), 4, 0), 4);
            assert_eq!(cbuf2str(&buf), "test");

            assert_eq!(cage2.close_syscall(clientsockfd1), 0);
            cage2.exit_syscall(EXIT_SUCCESS);
        });

        //client 2 connects to the server to send and recv data...
        let threadclient2 = interface::helper_thread(move || {
            let cage3 = interface::cagetable_getref(3);
            assert_eq!(cage3.close_syscall(serversockfd), 0);

            assert_eq!(cage3.connect_syscall(clientsockfd2, &socket), 0);
            barrier_clone2.wait();
            assert_eq!(cage3.send_syscall(clientsockfd2, str2cbuf("test"), 4, 0), 4);

            interface::sleep(interface::RustDuration::from_millis(1));

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
            cage3.exit_syscall(EXIT_SUCCESS);
        });
        barrier.wait();
        //acting as the server and processing the request
        // Server loop to handle connections and I/O
        //Check for any activity in any of the Input sockets...
        for _counter in 0..600 {
            working_set.copy_from(master_set);
            let select_result = cage.select_syscall(
                11,
                Some(working_set),
                Some(outputs),
                None,
                Some(interface::RustDuration::ZERO),
            );
            assert!(select_result >= 0);
            //Check for any activity in any of the Input sockets...
            //for sock in binputs {
            for sock in 0..FD_SET_MAX_FD {
                if !working_set.is_set(sock) {
                    continue;
                }
                //If the socket returned was listerner socket, then there's a new conn., so we accept it, and put the client socket in the list of Inputs.
                if sock == serversockfd {
                    let mut sockgarbage = interface::GenSockaddr::V4(interface::SockaddrV4::default());
                    let sockfd = cage.accept_syscall(sock as i32, &mut sockgarbage);
                    assert!(sockfd > 0);
                    master_set.set(sockfd);
                    outputs.set(sockfd);
                } else if sock == filefd {
                    //Write to a file...
                    assert_eq!(cage.write_syscall(sock as i32, str2cbuf("test"), 4), 4);
                    assert_eq!(cage.lseek_syscall(sock as i32, 0, SEEK_SET), 0);
                    master_set.clear(sock);
                } else {
                    //If the socket is in established conn., then we recv the data. If there's no data, then close the client socket.
                    let mut buf = sizecbuf(4);
                    let mut recvresult: i32;
                    loop {
                        recvresult = cage.recv_syscall(sock as i32, buf.as_mut_ptr(), 4, 0);
                        if recvresult != -libc::EINTR {
                            break; // if the error was EINTR, retry the syscall
                        }
                    }
                    if recvresult == 4 {
                        if cbuf2str(&buf) == "test" {
                            outputs.set(sock);
                            continue;
                        }
                    } else if recvresult == -libc::ECONNRESET {
                        println!("Connection reset by peer on socket {}", sock);
                        assert_eq!(cage.close_syscall(sock as i32), 0);
                        master_set.clear(sock);
                        outputs.clear(sock);
                    } else {
                        assert_eq!(recvresult, 0);
                        assert_eq!(cage.close_syscall(sock as i32), 0);
                        master_set.clear(sock);
                    }
                }
            }

            //for sock in boutputs {
            for sock in 0..FD_SET_MAX_FD {
                if !outputs.is_set(sock) {
                    continue;
                }
                if sock == filefd {
                    let mut buf = sizecbuf(4);
                    assert_eq!(cage.read_syscall(sock as i32, buf.as_mut_ptr(), 4), 4);
                    assert_eq!(cbuf2str(&buf), "test");
                    outputs.clear(sock);
                } else {
                    //Data is sent out this socket, it's no longer ready for writing remove this socket from writefd's.
                    assert_eq!(cage.send_syscall(sock as i32, str2cbuf("test"), 4, 0), 4);
                    outputs.clear(sock);
                }
            }
        }
        assert_eq!(cage.close_syscall(serversockfd), 0);

        threadclient1.join().unwrap();
        threadclient2.join().unwrap();

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_net_shutdown() {
        lindrustinit(0);
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
            assert_ne!(cage2.netshutdown_syscall(fd, SHUT_RDWR), 0); //should fail

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

    pub fn ut_lind_net_socket() {
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);

        let mut sockfd = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        let sockfd2 = cage.socket_syscall(AF_INET, SOCK_STREAM, IPPROTO_TCP);

        let sockfd3 = cage.socket_syscall(AF_INET, SOCK_DGRAM, 0);
        let sockfd4 = cage.socket_syscall(AF_INET, SOCK_DGRAM, IPPROTO_UDP);

        //checking that the fd's are correct
        assert!(sockfd > 0);
        assert!(sockfd2 > 0);
        assert!(sockfd3 > 0);
        assert!(sockfd4 > 0);

        //let's check an illegal operation...
        let sockfddomain = cage.socket_syscall(AF_UNIX, SOCK_DGRAM, 0);
        assert!(sockfddomain > 0);

        sockfd = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        assert!(sockfd > 0);

        assert_eq!(cage.close_syscall(sockfd), 0);
        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_net_socketoptions() {
        lindrustinit(0);
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

        //set and get some options:
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

        //linger...
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

        //check the options
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

        //reuseport...
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

        //check the options
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

        //keep alive...
        assert_eq!(
            cage.getsockopt_syscall(sockfd, SOL_SOCKET, SO_KEEPALIVE, &mut optstore),
            0
        );
        assert_eq!(optstore, 0);
        assert_eq!(
            cage.setsockopt_syscall(sockfd, SOL_SOCKET, SO_KEEPALIVE, 1),
            0
        );

        //check the options
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

        //check the options
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

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_net_socketpair() {
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
            loop {
                let result = cage2.recv_syscall(socketpair.sock2, buf.as_mut_ptr(), 10, 0);
                if result != -libc::EINTR {
                    break; // if the error was EINTR, retry the syscall
                }
            }
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
        loop {
            let result = cage.recv_syscall(socketpair.sock1, buf2.as_mut_ptr(), 15, 0);
            if result != -libc::EINTR {
                break; // if the error was EINTR, retry the syscall
            }
        }
        let str2 = cbuf2str(&buf2);
        assert_eq!(str2, "Socketpair Test");

        thread.join().unwrap();

        assert_eq!(cage.close_syscall(socketpair.sock1), 0);
        assert_eq!(cage.close_syscall(socketpair.sock2), 0);

        // end of the socket pair test (note we are only supporting AF_UNIX and TCP)

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_net_udp_bad_bind() {
        lindrustinit(0);
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
    pub fn ut_lind_net_udp_simple() {
        lindrustinit(0);
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

    pub fn ut_lind_net_udp_connect() {
        lindrustinit(0);
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

    pub fn ut_lind_net_gethostname() {
        //Assuming DEFAULT_HOSTNAME == "Lind" and change of hostname is not allowed
        lindrustinit(0);
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

        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    pub fn ut_lind_net_dns_rootserver_ping() {
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

        lindrustinit(0);
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

        //next we need to skip the null byte, qtype, and qclass to extract the main response payload
        let recordptr =
            dnsresp.as_ptr().wrapping_offset(nameptr as isize + 5) as *const DnsRecordAT;
        let record = unsafe { &*recordptr };
        let addr = u32::from_be(record.addr.s_addr);
        assert_eq!(addr, 0x23ac5973); //check that what is returned is the actual ip, 35.172.89.115
                                      //assert_eq!(record.addr.s_addr, 0x7359ac23); //check that what is returned is the actual ip, 35.172.89.115

        lindrustfinalize();
    }

    pub fn ut_lind_net_domain_socket() {
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

    /* Creates an epoll instance, registers the server socket and file descriptor with epoll, and then wait for events using
    epoll_wait_syscall(). It handles the events based on their types (EPOLLIN or EPOLLOUT) and performs the necessary operations
    like accepting new connections, sending/receiving data, and modifying the event flags */
    pub fn ut_lind_net_epoll() {
        lindrustinit(0);
        let cage = interface::cagetable_getref(1);

        let filefd = cage.open_syscall("/netepolltest.txt", O_CREAT | O_EXCL | O_RDWR, S_IRWXA);
        assert!(filefd > 0);

        let serversockfd = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        let clientsockfd1 = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        let clientsockfd2 = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        let port: u16 = generate_random_port();

        // Create and set up the file descriptor and sockets
        let sockaddr = interface::SockaddrV4 {
            sin_family: AF_INET as u16,
            sin_port: port.to_be(),
            sin_addr: interface::V4Addr {
                s_addr: u32::from_ne_bytes([127, 0, 0, 1]),
            },
            padding: 0,
        };
        let socket = interface::GenSockaddr::V4(sockaddr);
        assert_eq!(cage.bind_syscall(serversockfd, &socket), 0);
        assert_eq!(cage.listen_syscall(serversockfd, 4), 0);

        let mut event_list = vec![
            EpollEvent {
                events: EPOLLIN as u32,
                fd: serversockfd,
            },
            EpollEvent {
                events: EPOLLIN as u32,
                fd: filefd,
            },
        ];

        cage.fork_syscall(2);
        // Client 1 connects to the server to send and recv data
        let thread1 = interface::helper_thread(move || {
            interface::sleep(interface::RustDuration::from_millis(30));
            let cage2 = interface::cagetable_getref(2);
            // Connect to server and send data
            assert_eq!(cage2.connect_syscall(clientsockfd1, &socket), 0);
            assert_eq!(
                cage2.send_syscall(clientsockfd1, str2cbuf(&"test"), 4, 0),
                4
            );
            // Wait for data processing, give it a longer pause time so that it can process all of the data received
            interface::sleep(interface::RustDuration::from_millis(100));
            // Close the server socket and exit the thread
            assert_eq!(cage2.close_syscall(serversockfd), 0);
            cage2.exit_syscall(EXIT_SUCCESS);
        });

        cage.fork_syscall(3);
        // Client 2 connects to the server to send and recv data
        let thread2 = interface::helper_thread(move || {
            interface::sleep(interface::RustDuration::from_millis(45));
            let cage3 = interface::cagetable_getref(3);
            // Connect to server and send data
            assert_eq!(cage3.connect_syscall(clientsockfd2, &socket), 0);
            assert_eq!(
                cage3.send_syscall(clientsockfd2, str2cbuf(&"test"), 4, 0),
                4
            );

            interface::sleep(interface::RustDuration::from_millis(100));
            // Close the server socket and exit the thread
            assert_eq!(cage3.close_syscall(serversockfd), 0);
            cage3.exit_syscall(EXIT_SUCCESS);
        });

        // Acting as the server and processing the request
        let thread3 = interface::helper_thread(move || {
            let epfd = cage.epoll_create_syscall(1);
            assert!(epfd > 0);

            assert_eq!(
                cage.epoll_ctl_syscall(epfd, EPOLL_CTL_ADD, serversockfd, &mut event_list[0]),
                0
            );
            assert_eq!(
                cage.epoll_ctl_syscall(epfd, EPOLL_CTL_ADD, filefd, &mut event_list[1]),
                0
            );
            // Event processing loop
            for _counter in 0..600 {
                let num_events = cage.epoll_wait_syscall(
                    epfd,
                    &mut event_list,
                    1,
                    Some(interface::RustDuration::ZERO),
                );
                assert!(num_events >= 0);

                // Wait for events using epoll_wait_syscall
                for event in &mut event_list[..num_events as usize] {
                    // Check for any activity in the input socket and if there are events ready for reading
                    if event.events & (EPOLLIN as u32) != 0 {
                        // If the socket returned was listener socket, then there's a new connection
                        if event.fd == serversockfd {
                            // Handle new connections
                            let port: u16 = generate_random_port();
                            let sockaddr = interface::SockaddrV4 {
                                sin_family: AF_INET as u16,
                                sin_port: port.to_be(),
                                sin_addr: interface::V4Addr {
                                    s_addr: u32::from_ne_bytes([127, 0, 0, 1]),
                                },
                                padding: 0,
                            };
                            let mut addr = interface::GenSockaddr::V4(sockaddr); // 127.0.0.1 from bytes above
                            let newsockfd = cage.accept_syscall(serversockfd, &mut addr);
                            let event = interface::EpollEvent {
                                events: EPOLLIN as u32,
                                fd: newsockfd,
                            };
                            // Error raised to indicate that the socket file descriptor couldn't be added to the epoll instance
                            assert_eq!(
                                cage.epoll_ctl_syscall(epfd, EPOLL_CTL_ADD, newsockfd, &event),
                                0
                            );
                        } else if event.fd == filefd {
                            // Handle writing to the file
                            // Update
                            assert_eq!(cage.write_syscall(filefd, str2cbuf("test"), 4), 4);
                            assert_eq!(cage.lseek_syscall(filefd, 0, SEEK_SET), 0);
                            event.events = EPOLLOUT as u32;
                        } else {
                            // Handle receiving data from established connections
                            let mut buf = sizecbuf(4);
                            let recres = cage.recv_syscall(event.fd, buf.as_mut_ptr(), 4, 0);
                            assert_eq!(recres & !4, 0);
                            if recres == 4 {
                                assert_eq!(cbuf2str(&buf), "test");
                                event.events = EPOLLOUT as u32;
                            } else {
                                assert_eq!(cage.close_syscall(event.fd), 0);
                            }
                        }
                    }

                    if event.events & (EPOLLOUT as u32) != 0 {
                        // Check if there are events ready for writing
                        if event.fd == filefd {
                            // Handle reading from the file
                            let mut read_buf1 = sizecbuf(4);
                            assert_eq!(cage.read_syscall(filefd, read_buf1.as_mut_ptr(), 4), 4);
                            assert_eq!(cbuf2str(&read_buf1), "test");
                        } else {
                            // Handle sending data over connections
                            assert_eq!(cage.send_syscall(event.fd, str2cbuf(&"test"), 4, 0), 4);
                            event.events = EPOLLIN as u32;
                        }
                    }
                }
            }

            // Close the server socket and exit the thread
            assert_eq!(cage.close_syscall(serversockfd), 0);
            assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        });

        thread1.join().unwrap();
        thread2.join().unwrap();
        thread3.join().unwrap();

        lindrustfinalize();
    }

    pub fn ut_lind_net_writev() {
        lindrustinit(0);
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

        //creating a thread for the server so that the information can be sent between the two threads
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
