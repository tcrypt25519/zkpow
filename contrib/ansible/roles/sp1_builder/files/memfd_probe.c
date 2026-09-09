#define _GNU_SOURCE
#include <sys/mman.h>
#include <sys/syscall.h>
#include <unistd.h>
#include <stdio.h>

int main(void) {
    int fd = syscall(SYS_memfd_create, "sp1_droplet_test", 0);
    if (fd < 0) {
        perror("memfd_create failed");
        return 1;
    }
    close(fd);
    printf("[+] memfd_create syscall: functional\n");
    return 0;
}
