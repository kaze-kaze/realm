/* LD_PRELOAD for the realm under test only (T1).
 * While $POISON_FILE exists:
 *  - bind() of a UDP socket to port 0 fails with EADDRINUSE: what the kernel returns when no
 *    ephemeral port is left (bind(2)).
 *  - with $POISON_SEND set: sendmmsg() on a UDP socket that has no local port fails with EAGAIN
 *    (the implicit bind of an unbound socket, v3.2.0 path; same as noport.c).
 * Counters go to $SHIM_STAT. */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <fcntl.h>
#include <netinet/in.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/socket.h>
#include <unistd.h>

static atomic_long nbind, nsend;
static int statfd = -1;
static void dump(void) {
    char b[96];
    if (statfd < 0 && getenv("SHIM_STAT")) statfd = open(getenv("SHIM_STAT"), O_CREAT | O_WRONLY | O_TRUNC, 0644);
    int n = snprintf(b, sizeof b, "bind_refused=%ld send_refused=%ld\n", atomic_load(&nbind), atomic_load(&nsend));
    if (statfd >= 0) pwrite(statfd, b, n, 0);
}
static int poisoned(void) {
    const char *f = getenv("POISON_FILE");
    return f && access(f, F_OK) == 0;
}
static int is_udp(int fd) {
    int type = 0; socklen_t tl = sizeof type;
    return getsockopt(fd, SOL_SOCKET, SO_TYPE, &type, &tl) == 0 && type == SOCK_DGRAM;
}
static int port_of(const struct sockaddr *a) {
    if (a->sa_family == AF_INET) return ntohs(((const struct sockaddr_in *)a)->sin_port);
    if (a->sa_family == AF_INET6) return ntohs(((const struct sockaddr_in6 *)a)->sin6_port);
    return -1;
}
int bind(int fd, const struct sockaddr *a, socklen_t l) {
    static int (*real)(int, const struct sockaddr *, socklen_t);
    if (!real) real = dlsym(RTLD_NEXT, "bind");
    if (port_of(a) == 0 && is_udp(fd) && poisoned()) {
        atomic_fetch_add(&nbind, 1); dump();
        errno = EADDRINUSE; return -1;
    }
    return real(fd, a, l);
}
int sendmmsg(int fd, struct mmsghdr *m, unsigned int n, int flags) {
    static int (*real)(int, struct mmsghdr *, unsigned int, int);
    if (!real) real = dlsym(RTLD_NEXT, "sendmmsg");
    if (getenv("POISON_SEND") && poisoned()) {
        struct sockaddr_storage ss; socklen_t len = sizeof ss;
        if (getsockname(fd, (struct sockaddr *)&ss, &len) == 0 && port_of((struct sockaddr *)&ss) == 0) {
            atomic_fetch_add(&nsend, 1); dump();
            errno = EAGAIN; return -1;
        }
    }
    return real(fd, m, n, flags);
}
