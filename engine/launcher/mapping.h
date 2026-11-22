#ifndef RNX_LAUNCHER_MAPPING_H
#define RNX_LAUNCHER_MAPPING_H
#include <dlfcn.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/stat.h>
#include <sys/sysmacros.h>

static int owns_symbol(const struct stat *st, uintptr_t pc) {
	Dl_info owner;
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

#endif
