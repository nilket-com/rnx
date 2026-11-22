#ifndef RNX_LAUNCHER_IDENTITY_H
#define RNX_LAUNCHER_IDENTITY_H
#include <dlfcn.h>
#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <openssl/evp.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/sysmacros.h>
#include <unistd.h>

/* Check the executable mapping, not just the name recorded by the loader.
 * Opening the expected file once ties the identity check and hash together. */
#include "mapping.h"

static int verified_entry(uintptr_t entry) {
	char path[PATH_MAX];
	ssize_t n = readlink("/proc/self/exe", path, sizeof(path));
	if (n <= 0 || (size_t)n >= sizeof(path)) return 0;
	path[n] = 0;
	char *last = strrchr(path, '/');
	if (!last || (size_t)(last-path) + sizeof("/libengine_image.so") > sizeof(path)) return 0;
	strcpy(last, "/libengine_image.so");
	int fd = open(path, O_RDONLY | O_CLOEXEC | O_NOFOLLOW);
	if (fd < 0) return 0;
	struct stat st;
	EVP_MD_CTX *ctx = EVP_MD_CTX_new();
	int ok = ctx && !fstat(fd, &st) && S_ISREG(st.st_mode) && owns_symbol(&st, entry)
		&& EVP_DigestInit_ex(ctx, EVP_sha256(), NULL) == 1;
	unsigned char block[65536], digest[EVP_MAX_MD_SIZE];
	while (ok && (n = read(fd, block, sizeof(block))) != 0) {
		if (n < 0 && errno == EINTR) continue;
		ok = n > 0 && EVP_DigestUpdate(ctx, block, (size_t)n) == 1;
	}
	unsigned int length;
	ok = ok && EVP_DigestFinal_ex(ctx, digest, &length) == 1 && length == 32;
	EVP_MD_CTX_free(ctx);
	close(fd);
	if (!ok) return 0;
	char hex[65];
	for (size_t i=0; i<32; ++i) snprintf(hex+2*i, 3, "%02x", digest[i]);
	return !strcmp(hex, ENGINE_DIGEST);
}

#endif
