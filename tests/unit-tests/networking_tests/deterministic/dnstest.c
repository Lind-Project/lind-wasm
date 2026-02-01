#include <arpa/inet.h>
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/socket.h>

int main(void) {
    struct DnsHeader {
        uint16_t xid;
        uint16_t flags;
        uint16_t qdcount;
        uint16_t ancount;
        uint16_t nscount;
        uint16_t arcount;
    };

    struct DnsRecordAT {
        uint16_t compression;
        uint16_t typ;
        uint16_t clas;
        uint32_t ttl;
        uint16_t length;
        struct in_addr addr;
    } __attribute__((packed));

    int sockfd = socket(AF_INET, SOCK_DGRAM, 0);
    assert(sockfd >= 0);

    char *hostname = "\13engineering\03nyu\03edu\0";
    uint16_t dnstype = htons(1);
    uint16_t dnsclass = htons(1);

    int packetlen = sizeof(struct DnsHeader) + (int)strlen(hostname) + 1 + 2 + 2;
    char packvla[packetlen];
    int packetlenleft = packetlen;

    struct DnsHeader dnsh;
    dnsh.xid = htons(0x1234);
    dnsh.flags = htons(0x0100);
    dnsh.qdcount = htons(0x0001);
    dnsh.ancount = 0;
    dnsh.nscount = 0;
    dnsh.arcount = 0;

    memcpy(packvla, &dnsh, sizeof(struct DnsHeader));
    packetlenleft -= sizeof(struct DnsHeader);
    memcpy(packvla + packetlen - packetlenleft, hostname, strlen(hostname) + 1);
    packetlenleft -= (int)strlen(hostname) + 1;
    memcpy(packvla + packetlen - packetlenleft, &dnstype, 2);
    packetlenleft -= 2;
    memcpy(packvla + packetlen - packetlenleft, &dnsclass, 2);
    packetlenleft -= 2;
    assert(packetlenleft == 0);

    /* Canned DNS response: header + question (same name) + one A record 192.0.2.1 */
    static const unsigned char canned[] = {
        /* Header: xid=0x1234, flags=0x8180, qd=1, an=1, ns=0, ar=0 */
        0x12, 0x34, 0x81, 0x80, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
        /* Question: \x0b engineering \x03 nyu \x03 edu \x00 */
        0x0b, 'e','n','g','i','n','e','e','r','i','n','g', 0x03, 'n','y','u', 0x03, 'e','d','u', 0x00,
        /* qtype=1, qclass=1 */
        0x00, 0x01, 0x00, 0x01,
        /* Answer: ptr to 12, type A, class IN, ttl=60, rdlength=4, 192.0.2.1 */
        0xc0, 0x0c, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x3c, 0x00, 0x04,
        192, 0, 2, 1
    };

    const char *dnsresp = (const char *)canned;
    const struct DnsHeader *rdnsh = (const struct DnsHeader *)dnsresp;

    assert(!(ntohs(rdnsh->flags) & 0xf));

    const char *nameptr = dnsresp + sizeof(struct DnsHeader);
    while (*nameptr != 0) {
        nameptr += (unsigned char)*nameptr + 1;
    }

    const struct DnsRecordAT *recordptr = (const struct DnsRecordAT *)(nameptr + 5);
    assert(ntohs(recordptr->typ) == 1);
    assert(ntohs(recordptr->clas) == 1);
    assert(ntohs(recordptr->length) == 4);
    {
        const unsigned char *a = (const unsigned char *)&recordptr->addr;
        assert(a[0] == 192 && a[1] == 0 && a[2] == 2 && a[3] == 1);
    }

    printf("dnstest ok\n");
    return 0;
}
