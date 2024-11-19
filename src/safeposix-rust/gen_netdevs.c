#include <arpa/inet.h>
#include <sys/socket.h>
#include <ifaddrs.h>
#include <stdio.h>
#include <sys/ioctl.h>
#include <net/if.h>

int main ()
{
    struct ifaddrs *ifap, *ifa;
    struct sockaddr_in *sa, *ba, *na, *da;
    char *addr, *baddr, *naddr, *daddr;

    getifaddrs (&ifap);
    for (ifa = ifap; ifa; ifa = ifa->ifa_next) {
        if (ifa->ifa_addr && ifa->ifa_addr->sa_family==AF_INET) {
            sa = (struct sockaddr_in *) ifa->ifa_addr;
            addr = inet_ntoa(sa->sin_addr);
            na = (struct sockaddr_in *) ifa->ifa_netmask;
            naddr = inet_ntoa(na->sin_addr);
            ba = (struct sockaddr_in *) ifa->ifa_broadaddr;
            baddr = inet_ntoa(ba->sin_addr);

            printf("%s %d %s %s %s\n", ifa->ifa_name, ifa->ifa_flags, addr, naddr, baddr);
        }
    }

    freeifaddrs(ifap);
    return 0;
}
