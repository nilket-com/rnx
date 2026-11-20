#define _GNU_SOURCE
#include "engine_image.h"
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
static int owns_symbol(const struct stat *st) {
	Dl_info owner;
	uintptr_t pc = (uintptr_t)&rnx_engine_run_file_v1;
	if (!dladdr((void *)pc, &owner) || !owner.dli_fbase) return 0;
	FILE *maps = fopen("/proc/self/maps", "r");
	if (!maps) return 0;
	char *line = NULL;
	size_t size = 0;
	int matches = 0;
	while (getline(&line, &size, maps) >= 0) {
		unsigned long lo, hi, ino;
		unsigned int maj, min;
		char prot[5];
		if (sscanf(line, "%lx-%lx %4s %*x %x:%x %lu", &lo, &hi, prot,
				&maj, &min, &ino) == 6 && lo <= pc && pc < hi) {
			matches = prot[2] == 'x' && st->st_dev == makedev(maj, min) && st->st_ino == ino;
			break;
		}
	}
	free(line);
	fclose(maps);
	return matches;
}

static int verified(void) {
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
	int ok = ctx && !fstat(fd, &st) && S_ISREG(st.st_mode) && owns_symbol(&st)
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

struct registry {
#ifdef TAGGED
	struct { unsigned tag; int offset; } slots[2];
#else
	int offset;
#endif
	unsigned calls;
};
static int64_t invoke(void *context, int64_t x) {
	struct registry *r = context;
	++r->calls;
#ifdef TAGGED
	int delta = r->slots[1].offset;
#else
	int delta = r->offset;
#endif
	return (int64_t)((uint64_t)x + (unsigned)delta);
}

int main(int argc, char **argv) {
	if (argc < 3 || strcmp(argv[1], "run")) return RNX_INVALID;
	struct registry registry = {
#ifdef TAGGED
		.slots = {{2,101},{1,3}},
#else
		.offset = 2,
#endif
	};
	uint8_t buffer[4096];
	size_t written = 0, needed = 0;
	struct rnx_request r = {.version=1, .size=sizeof(r), .flags=RNX_NATIVE, .fuel=2000000,
		.apply=invoke, .context=&registry, .buffer=buffer, .capacity=sizeof(buffer),
		.written=&written, .needed=&needed};
	for (int i=2; i<argc-1; ++i) {
		if (!strcmp(argv[i], "--native=off")) r.flags &= ~RNX_NATIVE;
		else if (!strcmp(argv[i], "--native=on")) r.flags |= RNX_NATIVE;
		else if (!strcmp(argv[i], "--debug-engine")) r.flags |= RNX_DEBUG;
		else if (!strcmp(argv[i], "--budget") && ++i < argc-1) {
			char *end;
			errno=0;
			if (argv[i][0]<'0' || argv[i][0]>'9') return RNX_INVALID;
			r.fuel=strtoull(argv[i], &end, 10);
			if (*end || errno || !r.fuel || r.fuel==UINT64_MAX) return RNX_INVALID;
		} else return RNX_INVALID;
	}
	r.path=(const uint8_t *)argv[argc-1];
	r.path_len=strlen(argv[argc-1]);
	if (!verified()) {
		fputs("engine image identity mismatch\n",stderr);
		return RNX_INVALID;
	}
	int32_t status=rnx_engine_run_file_v1(&r);
	if (written) fwrite(buffer,1,written,stderr);
	if (r.flags & RNX_DEBUG) fprintf(stderr,"rnx: callback-count=%u\n", registry.calls);
	return status;
}
