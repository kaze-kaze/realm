/* Test shim, loaded only into the relay under test: while $POISON_FILE exists, a
 * sendmmsg on a socket that has no local port yet fails with EAGAIN, which is what
 * the kernel returns from the implicit bind when no ephemeral port is left
 * (send(2): EAGAIN, "all port numbers in the ephemeral port range are in use"). */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <stdlib.h>
#include <unistd.h>
#include <sys/socket.h>
#include <netinet/in.h>

static int (*real_sendmmsg)(int, struct mmsghdr *, unsigned int, int);

int sendmmsg(int fd, struct mmsghdr *msgs, unsigned int n, int flags) {
    if (!real_sendmmsg) real_sendmmsg = dlsym(RTLD_NEXT, "sendmmsg");
    const char *flag = getenv("POISON_FILE");
    if (flag && access(flag, F_OK) == 0) {
        struct sockaddr_storage ss; socklen_t len = sizeof ss;
        if (getsockname(fd, (struct sockaddr *)&ss, &len) == 0) {
            int port = -1;
            if (ss.ss_family == AF_INET) port = ntohs(((struct sockaddr_in *)&ss)->sin_port);
            if (ss.ss_family == AF_INET6) port = ntohs(((struct sockaddr_in6 *)&ss)->sin6_port);
            if (port == 0) { errno = EAGAIN; return -1; }
        }
    }
    return real_sendmmsg(fd, msgs, n, flags);
}
