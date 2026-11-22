#define _GNU_SOURCE
#include "../mapping.h"
#include "../../image/engine_image.h"
#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <string.h>
#include <unistd.h>

#define IMAGE_FD 9
static int report_fd;
static int memfd;

static int descriptor(const char *text) {
	if (!text || *text < '0' || *text > '9') return -1;
	char *end;
	errno = 0;
	long n = strtol(text, &end, 10);
	return *end || errno || n > INT_MAX ? -1 : (int)n;
}

static void emit(const uint8_t *p, size_t n) {
	while (n) {
		ssize_t written = write(report_fd, p, n);
		if (written < 0 && errno == EINTR) continue;
		if (written <= 0) { perror("rnx: route evidence"); _exit(1); }
		p += written;
		n -= (size_t)written;
	}
}

static void route(const uint8_t *p, size_t n) {
	if (memfd && n && p[0] == '{') {
		static const uint8_t prefix[] = "{\"storage\":\"memfd\",";
		emit(prefix, sizeof(prefix) - 1);
		++p;
		--n;
	}
	emit(p, n);
	emit((const uint8_t *)"\n", 1);
}

static int names_member(int directory, const char *name, const struct stat *opened) {
	struct stat named;
	return fstatat(directory, name, &named, AT_SYMLINK_NOFOLLOW) == 0 &&
		S_ISREG(named.st_mode) && opened->st_dev == named.st_dev && opened->st_ino == named.st_ino;
}

static int sealed(int fd) {
	const int required = F_SEAL_WRITE | F_SEAL_GROW | F_SEAL_SHRINK | F_SEAL_SEAL;
	int seals = fcntl(fd, F_GET_SEALS);
	return seals >= 0 && (seals & required) == required;
}

int main(int argc, char **argv) {
	struct stat st, executable;
	char *separator = argc >= 3 ? strchr(argv[1], ',') : NULL;
	if (!separator) return 1;
	*separator = 0;
	memfd = strcmp(argv[1], "-1") == 0;
	int directory = descriptor(argv[1]);
	char *tail = strchr(separator + 1, ',');
	if (!tail) return 1;
	*tail = 0;
	int executable_fd = descriptor(separator + 1);
	int original = descriptor(tail + 1);
	if (fcntl(IMAGE_FD, F_SETFD, FD_CLOEXEC) != 0 ||
		fstat(IMAGE_FD, &st) || fstat(executable_fd, &executable) ||
		(memfd ? (!sealed(IMAGE_FD) || !sealed(executable_fd)) :
		 (directory <= IMAGE_FD || !names_member(directory, "engine", &st) ||
		  !names_member(directory, "helper", &executable))) ||
		!owns_symbol(&executable, (uintptr_t)&main) ||
		!owns_symbol(&st, (uintptr_t)&rnx_engine_stock_cli_v5)) {
		fputs("rnx: installed engine mapping mismatch\n", stderr);
		return 1;
	}
	// argv[1] carries the private handoff. Rust skips it before decoding the
	// command line. Restore any inheritable descriptor displaced by exec.
	if (!memfd) close(directory);
	close(executable_fd);
	if (original >= 0) {
		if (original < 3 || original == IMAGE_FD || dup2(original, IMAGE_FD) < 0) {
			fputs("rnx: cannot restore inherited descriptor\n", stderr);
			return 1;
		}
		close(original);
	} else {
		close(IMAGE_FD);
	}
	const char *report = getenv("RNX_FILE_ROUTE_FD");
	if (!report) return rnx_engine_stock_cli_v5(argc, argv, NULL);
	report_fd = descriptor(report);
	if (report_fd < 3 || fcntl(report_fd, F_GETFD) < 0) {
		fputs("rnx: invalid file route descriptor\n", stderr);
		return 1;
	}
	return rnx_engine_stock_cli_v5(argc, argv, route);
}
