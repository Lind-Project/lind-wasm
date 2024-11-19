// Network related constants
#![allow(dead_code)]
#![allow(non_upper_case_globals)]

use crate::interface;

//used for gethostname syscall
pub const DEFAULT_HOSTNAME: &str = "Lind";
pub const BLOCK_TIME: interface::RustDuration = interface::RustDuration::from_micros(100);

pub const UDSOCK_CAPACITY: usize = 212992;

// Define constants using static or const
// Imported into net_calls file

pub const SOCK_STREAM: i32 = 1; //stream socket
pub const SOCK_DGRAM: i32 = 2; //datagram socket
pub const SOCK_RAW: i32 = 3; //raw protocol interface
pub const SOCK_RDM: i32 = 4; //reliably delivered message
pub const SOCK_SEQPACKET: i32 = 5; //sequenced packet stream
pub const SOCK_CLOEXEC: i32 = 0o02000000; // Atomically set close-on-exec
pub const SOCK_NONBLOCK: i32 = 0o00004000; // Mark as non-blocking

/* Supported address families. */
pub const AF_UNSPEC: i32 = 0;
pub const AF_UNIX: i32 = 1; /* Unix domain sockets   */
pub const AF_LOCAL: i32 = 1; /* POSIX name for AF_UNIX */
pub const AF_INET: i32 = 2; /* Internet IP Protocol  */
pub const AF_AX25: i32 = 3; /* Amateur Radio AX.25   */
pub const AF_IPX: i32 = 4; /* Novell IPX    */
pub const AF_APPLETALK: i32 = 5; /* AppleTalk DDP   */
pub const AF_NETROM: i32 = 6; /* Amateur Radio NET/ROM  */
pub const AF_BRIDGE: i32 = 7; /* Multiprotocol bridge  */
pub const AF_ATMPVC: i32 = 8; /* ATM PVCs   */
pub const AF_X25: i32 = 9; /* Reserved for X.25 project  */
pub const AF_INET6: i32 = 10; /* IP version 6   */
pub const AF_ROSE: i32 = 11; /* Amateur Radio X.25 PLP */
pub const AF_DECnet: i32 = 12; /* Reserved for DECnet project */
pub const AF_NETBEUI: i32 = 13; /* Reserved for 802.2LLC project*/
pub const AF_SECURITY: i32 = 14; /* Security callback pseudo AF */
pub const AF_KEY: i32 = 15; /* PF_KEY key management API */
pub const AF_NETLINK: i32 = 16;
pub const AF_ROUTE: i32 = AF_NETLINK; /* Alias to emulate 4.4BSD */
pub const AF_PACKET: i32 = 17; /* Packet family  */
pub const AF_ASH: i32 = 18; /* Ash    */
pub const AF_ECONET: i32 = 19; /* Acorn Econet   */
pub const AF_ATMSVC: i32 = 20; /* ATM SVCs   */
pub const AF_RDS: i32 = 21; /* RDS sockets    */
pub const AF_SNA: i32 = 22; /* Linux SNA Project (nutters!) */
pub const AF_IRDA: i32 = 23; /* IRDA sockets   */
pub const AF_PPPOX: i32 = 24; /* PPPoX sockets  */
pub const AF_WANPIPE: i32 = 25; /* Wanpipe API Sockets */
pub const AF_LLC: i32 = 26; /* Linux LLC   */
pub const AF_IB: i32 = 27; /* Native InfiniBand address */
pub const AF_MPLS: i32 = 28; /* MPLS */
pub const AF_CAN: i32 = 29; /* Controller Area Network      */
pub const AF_TIPC: i32 = 30; /* TIPC sockets   */
pub const AF_BLUETOOTH: i32 = 31; /* Bluetooth sockets   */
pub const AF_IUCV: i32 = 32; /* IUCV sockets   */
pub const AF_RXRPC: i32 = 33; /* RxRPC sockets   */
pub const AF_ISDN: i32 = 34; /* mISDN sockets   */
pub const AF_PHONET: i32 = 35; /* Phonet sockets  */
pub const AF_IEEE802154: i32 = 36; /* IEEE802154 sockets  */
pub const AF_CAIF: i32 = 37; /* CAIF sockets   */
pub const AF_ALG: i32 = 38; /* Algorithm sockets  */
pub const AF_NFC: i32 = 39; /* NFC sockets   */
pub const AF_VSOCK: i32 = 40; /* vSockets   */
pub const AF_KCM: i32 = 41; /* Kernel Connection Multiplexor*/
pub const AF_QIPCRTR: i32 = 42; /* Qualcomm IPC Router          */
pub const AF_SMC: i32 = 43; /* smc sockets: reserve number for
                             * PF_SMC protocol family that
                             * reuses AF_INET address family
                             */
pub const AF_XDP: i32 = 44; /* XDP sockets   */
pub const AF_MCTP: i32 = 45; /* Management component
                              * transport protocol
                              */

pub const AF_MAX: i32 = 46; /* For now.. */

/* Protocol families, same as address families. */
pub const PF_UNSPEC: i32 = AF_UNSPEC;
pub const PF_UNIX: i32 = AF_UNIX;
pub const PF_LOCAL: i32 = AF_LOCAL;
pub const PF_INET: i32 = AF_INET;
pub const PF_AX25: i32 = AF_AX25;
pub const PF_IPX: i32 = AF_IPX;
pub const PF_APPLETALK: i32 = AF_APPLETALK;
pub const PF_NETROM: i32 = AF_NETROM;
pub const PF_BRIDGE: i32 = AF_BRIDGE;
pub const PF_ATMPVC: i32 = AF_ATMPVC;
pub const PF_X25: i32 = AF_X25;
pub const PF_INET6: i32 = AF_INET6;
pub const PF_ROSE: i32 = AF_ROSE;
pub const PF_DECnet: i32 = AF_DECnet;
pub const PF_NETBEUI: i32 = AF_NETBEUI;
pub const PF_SECURITY: i32 = AF_SECURITY;
pub const PF_KEY: i32 = AF_KEY;
pub const PF_NETLINK: i32 = AF_NETLINK;
pub const PF_ROUTE: i32 = AF_ROUTE;
pub const PF_PACKET: i32 = AF_PACKET;
pub const PF_ASH: i32 = AF_ASH;
pub const PF_ECONET: i32 = AF_ECONET;
pub const PF_ATMSVC: i32 = AF_ATMSVC;
pub const PF_RDS: i32 = AF_RDS;
pub const PF_SNA: i32 = AF_SNA;
pub const PF_IRDA: i32 = AF_IRDA;
pub const PF_PPPOX: i32 = AF_PPPOX;
pub const PF_WANPIPE: i32 = AF_WANPIPE;
pub const PF_LLC: i32 = AF_LLC;
pub const PF_IB: i32 = AF_IB;
pub const PF_MPLS: i32 = AF_MPLS;
pub const PF_CAN: i32 = AF_CAN;
pub const PF_TIPC: i32 = AF_TIPC;
pub const PF_BLUETOOTH: i32 = AF_BLUETOOTH;
pub const PF_IUCV: i32 = AF_IUCV;
pub const PF_RXRPC: i32 = AF_RXRPC;
pub const PF_ISDN: i32 = AF_ISDN;
pub const PF_PHONET: i32 = AF_PHONET;
pub const PF_IEEE802154: i32 = AF_IEEE802154;
pub const PF_CAIF: i32 = AF_CAIF;
pub const PF_ALG: i32 = AF_ALG;
pub const PF_NFC: i32 = AF_NFC;
pub const PF_VSOCK: i32 = AF_VSOCK;
pub const PF_KCM: i32 = AF_KCM;
pub const PF_QIPCRTR: i32 = AF_QIPCRTR;
pub const PF_SMC: i32 = AF_SMC;
pub const PF_XDP: i32 = AF_XDP;
pub const PF_MCTP: i32 = AF_MCTP;
pub const PF_MAX: i32 = AF_MAX;

// protocols...

pub const IPPROTO_IP: i32 = 0; // dummy for IP
pub const IPPROTO_ICMP: i32 = 1; // control message protocol
pub const IPPROTO_IGMP: i32 = 2; // group mgmt protocol
pub const IPPROTO_GGP: i32 = 3; // gateway^2 (deprecated)
pub const IPPROTO_IPV4: i32 = 4; // IPv4 encapsulation
pub const IPPROTO_IPIP: i32 = IPPROTO_IPV4; // for compatibility
pub const IPPROTO_TCP: i32 = 6; // tcp
pub const IPPROTO_ST: i32 = 7; // Stream protocol II
pub const IPPROTO_EGP: i32 = 8; // exterior gateway protocol
pub const IPPROTO_PIGP: i32 = 9; // private interior gateway
pub const IPPROTO_RCCMON: i32 = 10; // BBN RCC Monitoring
pub const IPPROTO_NVPII: i32 = 11; // network voice protocol
pub const IPPROTO_PUP: i32 = 12; // pup
pub const IPPROTO_ARGUS: i32 = 13; // Argus
pub const IPPROTO_EMCON: i32 = 14; // EMCON
pub const IPPROTO_XNET: i32 = 15; // Cross Net Debugger
pub const IPPROTO_CHAOS: i32 = 16; // Chaos
pub const IPPROTO_UDP: i32 = 17; // user datagram protocol
pub const IPPROTO_MUX: i32 = 18; // Multiplexing
pub const IPPROTO_MEAS: i32 = 19; // DCN Measurement Subsystems
pub const IPPROTO_HMP: i32 = 20; // Host Monitoring
pub const IPPROTO_PRM: i32 = 21; // Packet Radio Measurement
pub const IPPROTO_IDP: i32 = 22; // xns idp
pub const IPPROTO_TRUNK1: i32 = 23; // Trunk-1
pub const IPPROTO_TRUNK2: i32 = 24; // Trunk-2
pub const IPPROTO_LEAF1: i32 = 25; // Leaf-1
pub const IPPROTO_LEAF2: i32 = 26; // Leaf-2
pub const IPPROTO_RDP: i32 = 27; // Reliable Data
pub const IPPROTO_IRTP: i32 = 28; // Reliable Transaction
pub const IPPROTO_TP: i32 = 29; // tp-4 w/ class negotiation
pub const IPPROTO_BLT: i32 = 30; // Bulk Data Transfer
pub const IPPROTO_NSP: i32 = 31; // Network Services
pub const IPPROTO_INP: i32 = 32; // Merit Internodal
pub const IPPROTO_SEP: i32 = 33; // Sequential Exchange
pub const IPPROTO_3PC: i32 = 34; // Third Party Connect
pub const IPPROTO_IDPR: i32 = 35; // InterDomain Policy Routing
pub const IPPROTO_XTP: i32 = 36; // XTP
pub const IPPROTO_DDP: i32 = 37; // Datagram Delivery
pub const IPPROTO_CMTP: i32 = 38; // Control Message Transport
pub const IPPROTO_TPXX: i32 = 39; // TP++ Transport
pub const IPPROTO_IL: i32 = 40; // IL transport protocol
pub const IPPROTO_IPV6: i32 = 41; // IP6 header
pub const IPPROTO_SDRP: i32 = 42; // Source Demand Routing
pub const IPPROTO_ROUTING: i32 = 43; // IP6 routing header
pub const IPPROTO_FRAGMENT: i32 = 44; // IP6 fragmentation header
pub const IPPROTO_IDRP: i32 = 45; // InterDomain Routing
pub const IPPROTO_RSVP: i32 = 46; // resource reservation
pub const IPPROTO_GRE: i32 = 47; // General Routing Encap.
pub const IPPROTO_MHRP: i32 = 48; // Mobile Host Routing
pub const IPPROTO_BHA: i32 = 49; // BHA
pub const IPPROTO_ESP: i32 = 50; // IP6 Encap Sec. Payload
pub const IPPROTO_AH: i32 = 51; // IP6 Auth Header
pub const IPPROTO_INLSP: i32 = 52; // Integ. Net Layer Security
pub const IPPROTO_SWIPE: i32 = 53; // IP with encryption
pub const IPPROTO_NHRP: i32 = 54; // Next Hop Resolution
                                  // 55-57: Unassigned
pub const IPPROTO_ICMPV6: i32 = 58; // ICMP6
pub const IPPROTO_NONE: i32 = 59; // IP6 no next header
pub const IPPROTO_DSTOPTS: i32 = 60; // IP6 destination option
pub const IPPROTO_AHIP: i32 = 61; // any host internal protocol
pub const IPPROTO_CFTP: i32 = 62; // CFTP
pub const IPPROTO_HELLO: i32 = 63; // "hello" routing protocol
pub const IPPROTO_SATEXPAK: i32 = 64; // SATNET/Backroom EXPAK
pub const IPPROTO_KRYPTOLAN: i32 = 65; // Kryptolan
pub const IPPROTO_RVD: i32 = 66; // Remote Virtual Disk
pub const IPPROTO_IPPC: i32 = 67; // Pluribus Packet Core
pub const IPPROTO_ADFS: i32 = 68; // Any distributed FS
pub const IPPROTO_SATMON: i32 = 69; // Satnet Monitoring
pub const IPPROTO_VISA: i32 = 70; // VISA Protocol
pub const IPPROTO_IPCV: i32 = 71; // Packet Core Utility
pub const IPPROTO_CPNX: i32 = 72; // Comp. Prot. Net. Executive
pub const IPPROTO_CPHB: i32 = 73; // Comp. Prot. HeartBeat
pub const IPPROTO_WSN: i32 = 74; // Wang Span Network
pub const IPPROTO_PVP: i32 = 75; // Packet Video Protocol
pub const IPPROTO_BRSATMON: i32 = 76; // BackRoom SATNET Monitoring
pub const IPPROTO_ND: i32 = 77; // Sun net disk proto (temp.)
pub const IPPROTO_WBMON: i32 = 78; // WIDEBAND Monitoring
pub const IPPROTO_WBEXPAK: i32 = 79; // WIDEBAND EXPAK
pub const IPPROTO_EON: i32 = 80; // ISO cnlp
pub const IPPROTO_VMTP: i32 = 81; // VMTP
pub const IPPROTO_SVMTP: i32 = 82; // Secure VMTP
pub const IPPROTO_VINES: i32 = 83; // Banyon VINES
pub const IPPROTO_TTP: i32 = 84; // TTP
pub const IPPROTO_IGP: i32 = 85; // NSFNET-IGP
pub const IPPROTO_DGP: i32 = 86; // dissimilar gateway prot.
pub const IPPROTO_TCF: i32 = 87; // TCF
pub const IPPROTO_IGRP: i32 = 88; // Cisco/GXS IGRP
pub const IPPROTO_OSPFIGP: i32 = 89; // OSPFIGP
pub const IPPROTO_SRPC: i32 = 90; // Strite RPC protocol
pub const IPPROTO_LARP: i32 = 91; // Locus Address Resoloution
pub const IPPROTO_MTP: i32 = 92; // Multicast Transport
pub const IPPROTO_AX25: i32 = 93; // AX.25 Frames
pub const IPPROTO_IPEIP: i32 = 94; // IP encapsulated in IP
pub const IPPROTO_MICP: i32 = 95; // Mobile Int.ing control
pub const IPPROTO_SCCSP: i32 = 96; // Semaphore Comm. security
pub const IPPROTO_ETHERIP: i32 = 97; // Ethernet IP encapsulation
pub const IPPROTO_ENCAP: i32 = 98; // encapsulation header
pub const IPPROTO_APES: i32 = 99; // any private encr. scheme
pub const IPPROTO_GMTP: i32 = 100; // GMTP
pub const IPPROTO_PIM: i32 = 103; // Protocol Independent Mcast
pub const IPPROTO_IPCOMP: i32 = 108; // payload compression (IPComp)
pub const IPPROTO_PGM: i32 = 113; // PGM
pub const IPPROTO_SCTP: i32 = 132; // SCTP
pub const IPPROTO_DIVERT: i32 = 254; // divert pseudo-protocol
pub const IPPROTO_RAW: i32 = 255; // raw IP packet
pub const IPPROTO_MAX: i32 = 256;
// last return value of *_input(), meaning "all job for this pkt is done".
pub const IPPROTO_DONE: i32 = 257;

pub const MSG_OOB: i32 = 1;
pub const MSG_PEEK: i32 = 2;
pub const MSG_DONTROUTE: i32 = 4;
pub const MSG_TRYHARD: i32 = 4; /* Synonym for MSG_DONTROUTE for DECnet */
pub const MSG_CTRUNC: i32 = 8;
pub const MSG_PROBE: i32 = 0x10; /* Do not send. Only probe path f.e. for MTU */
pub const MSG_TRUNC: i32 = 0x20;
pub const MSG_DONTWAIT: i32 = 0x40; /* Nonblocking io		 */
pub const MSG_EOR: i32 = 0x80; /* End of record */
pub const MSG_WAITALL: i32 = 0x100; /* Wait for a full request */
pub const MSG_FIN: i32 = 0x200;
pub const MSG_SYN: i32 = 0x400;
pub const MSG_CONFIRM: i32 = 0x800; /* Confirm path validity */
pub const MSG_RST: i32 = 0x1000;
pub const MSG_ERRQUEUE: i32 = 0x2000; /* Fetch message from error queue */
pub const MSG_NOSIGNAL: i32 = 0x4000; /* Do not generate SIGPIPE */
pub const MSG_MORE: i32 = 0x8000; /* Sender will send more */
pub const MSG_WAITFORONE: i32 = 0x10000; /* recvmmsg(): block until 1+ packets avail */
pub const MSG_SENDPAGE_NOPOLICY: i32 = 0x10000; /* sendpage() internal : do no apply policy */
pub const MSG_SENDPAGE_NOTLAST: i32 = 0x20000; /* sendpage() internal : not the last page */
pub const MSG_BATCH: i32 = 0x40000; /* sendmmsg(): more messages coming */
pub const MSG_EOF: i32 = MSG_FIN;
pub const MSG_NO_SHARED_FRAGS: i32 = 0x80000; /* sendpage() internal : page frags are not shared */
pub const MSG_SENDPAGE_DECRYPTED: i32 = 0x100000; /* sendpage() internal : page may carry
                                                   * plain text and require encryption
                                                   */

//shutdown
pub const SHUT_RD: i32 = 0;
pub const SHUT_WR: i32 = 1;
pub const SHUT_RDWR: i32 = 2;

////////////////////// setsockopt / getsockopt...
pub const SOL_SOCKET: i32 = 1;

pub const SO_DEBUG: i32 = 1;
pub const SO_REUSEADDR: i32 = 2;
pub const SO_TYPE: i32 = 3;
pub const SO_ERROR: i32 = 4;
pub const SO_DONTROUTE: i32 = 5;
pub const SO_BROADCAST: i32 = 6;
pub const SO_SNDBUF: i32 = 7;
pub const SO_RCVBUF: i32 = 8;
pub const SO_SNDBUFFORCE: i32 = 32;
pub const SO_RCVBUFFORCE: i32 = 33;
pub const SO_KEEPALIVE: i32 = 9;
pub const SO_OOBINLINE: i32 = 10;
pub const SO_NO_CHECK: i32 = 11;
pub const SO_PRIORITY: i32 = 12;
pub const SO_LINGER: i32 = 13;
pub const SO_BSDCOMPAT: i32 = 14;
pub const SO_REUSEPORT: i32 = 15;
pub const SO_PASSCRED: i32 = 16;
pub const SO_PEERCRED: i32 = 17;
pub const SO_RCVLOWAT: i32 = 18;
pub const SO_SNDLOWAT: i32 = 19;
pub const SO_RCVTIMEO_OLD: i32 = 20;
pub const SO_SNDTIMEO_OLD: i32 = 21;
pub const SO_PEERNAME: i32 = 28;
pub const SO_ACCEPTCONN: i32 = 30;

// pub const SO_SECURITY_AUTHENTICATION: i32 = 22;
// pub const SO_SECURITY_ENCRYPTION_TRANSPORT: i32 = 23;
// pub const SO_SECURITY_ENCRYPTION_NETWORK: i32 = 24;

// pub const SO_BINDTODEVICE: i32 = 25;

// /* Socket filtering */
// pub const SO_ATTACH_FILTER: i32 = 26;
// pub const SO_DETACH_FILTER: i32 = 27;

// pub const SO_TIMESTAMP: i32 = 29;
// pub const SCM_TIMESTAMP: i32 = SO_TIMESTAMP;

// pub const SO_PEERSEC: i32 = 31;
// pub const SO_PASSSEC: i32 = 34;
// pub const SO_TIMESTAMPNS: i32 = 35;
// pub const SCM_TIMESTAMPNS: i32 = SO_TIMESTAMPNS;

// pub const SO_MARK: i32 = 36;

// pub const SO_TIMESTAMPING: i32 = 37;
// pub const SCM_TIMESTAMPING: i32 = SO_TIMESTAMPING;

// pub const SO_PROTOCOL: i32 = 38;
// pub const SO_DOMAIN: i32 = 39;

// pub const SO_RXQ_OVFL: i32 = 40;

// Use this to specify options on a socket. Use the protocol with setsockopt
// to specify something for all sockets with a protocol
pub const SOL_TCP: i32 = IPPROTO_TCP;
pub const SOL_UDP: i32 = IPPROTO_UDP;

pub const TCP_NODELAY: i32 = 0x01; // don't delay send to coalesce packets
pub const TCP_MAXSEG: i32 = 0x02; // set maximum segment size
pub const TCP_NOPUSH: i32 = 0x04; // don't push last block of write
pub const TCP_NOOPT: i32 = 0x08; // don't use TCP options
pub const TCP_KEEPALIVE: i32 = 0x10; // idle time used when SO_KEEPALIVE is enabled
pub const TCP_CONNECTIONTIMEOUT: i32 = 0x20; // connection timeout
pub const PERSIST_TIMEOUT: i32 = 0x40; // time after which a connection in persist timeout
                                       // will terminate.
                                       // see draft-ananth-tcpm-persist-02.txt
pub const TCP_RXT_CONNDROPTIME: i32 = 0x80; // time after which tcp retransmissions will be
                                            // stopped and the connection will be dropped
pub const TCP_RXT_FINDROP: i32 = 0x100; // When set, a connection is dropped after 3 FINs

pub const MINSOCKOBJID: i32 = 0;
pub const MAXSOCKOBJID: i32 = 1024;

//POLL CONSTANTS
pub const POLLIN: i16 = 0o1; // There is data to read.
pub const POLLPRI: i16 = 0o2; //There is urgent data to read.
pub const POLLOUT: i16 = 0o4; // Writing now will not block.
pub const POLLERR: i16 = 0o10; // Error condition.
pub const POLLHUP: i16 = 0o20; // Hung up.
pub const POLLNVAL: i16 = 0o40; // Invalid polling request.

//EPOLL CONSTANTS
pub const EPOLLIN: i32 = 0x001;
pub const EPOLLPRI: i32 = 0x002;
pub const EPOLLOUT: i32 = 0x004;
pub const EPOLLRDNORM: i32 = 0x040;
pub const EPOLLRDBAND: i32 = 0x080;
pub const EPOLLWRNORM: i32 = 0x100;
pub const EPOLLWRBAND: i32 = 0x200;
pub const EPOLLMSG: i32 = 0x400;
pub const EPOLLERR: i32 = 0x008;
pub const EPOLLHUP: i32 = 0x010;
pub const EPOLLRDHUP: i32 = 0x2000;
pub const EPOLLWAKEUP: i32 = 1 << 29;
pub const EPOLLONESHOT: i32 = 1 << 30;
pub const EPOLLET: i32 = 1 << 31;

pub const EPOLL_CTL_ADD: i32 = 1;
pub const EPOLL_CTL_DEL: i32 = 2;
pub const EPOLL_CTL_MOD: i32 = 3;

pub const FD_SET_MAX_FD: i32 = 1024;

//for internal use
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ConnState {
    NOTCONNECTED,
    CONNECTED,
    CONNRDONLY,
    CONNWRONLY,
    LISTEN,
    INPROGRESS,
}
