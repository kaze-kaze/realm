/* LD_PRELOAD for the realm under test only.
 * - bind(): the UDP socket bound to port $SHIM_PORT gets SO_SNDBUF = $SHIM_SNDBUF. Nothing else changes:
 *   EAGAIN, EPOLLOUT and wake-ups all come from the real kernel on a real UDP socket.
 * - sendmmsg(): counts calls and EAGAIN results on that socket, written to $SHIM_STAT. */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <fcntl.h>
#include <netinet/in.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <unistd.h>

static int lfd = -1, statfd = -1;
static atomic_long calls, eagain;

static void dump(void) {
    char b[96];
    int n = snprintf(b, sizeof b, "calls=%ld eagain=%ld\n", atomic_load(&calls), atomic_load(&eagain));
    if (statfd >= 0) pwrite(statfd, b, n, 0);
}

int bind(int fd, const struct sockaddr *a, socklen_t l) {
    static int (*real)(int, const struct sockaddr *, socklen_t);
    if (!real) real = dlsym(RTLD_NEXT, "bind");
    int r = real(fd, a, l);
    const char *p = getenv("SHIM_PORT"), *s = getenv("SHIM_SNDBUF");
    int type = 0; socklen_t tl = sizeof type;
    if (r == 0 && p && a->sa_family == AF_INET && ntohs(((const struct sockaddr_in *)a)->sin_port) == atoi(p)
        && getsockopt(fd, SOL_SOCKET, SO_TYPE, &type, &tl) == 0 && type == SOCK_DGRAM) {
        if (s) { int v = atoi(s); setsockopt(fd, SOL_SOCKET, SO_SNDBUF, &v, sizeof v); }
        lfd = fd;
        if (getenv("SHIM_STAT")) statfd = open(getenv("SHIM_STAT"), O_CREAT | O_WRONLY | O_TRUNC, 0644);
        dump();
    }
    return r;
}

int sendmmsg(int fd, struct mmsghdr *m, unsigned int n, int flags) {
    static int (*real)(int, struct mmsghdr *, unsigned int, int);
    if (!real) real = dlsym(RTLD_NEXT, "sendmmsg");
    int r = real(fd, m, n, flags);
    if (fd == lfd) {
        int e = errno;
        long c = atomic_fetch_add(&calls, 1);
        if (r < 0 && e == EAGAIN) { atomic_fetch_add(&eagain, 1); dump(); }
        else if ((c & 1023) == 0) dump();
        errno = e;
    }
    return r;
}
