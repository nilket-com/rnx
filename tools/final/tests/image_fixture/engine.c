typedef int (*callback)(int);

__attribute__((visibility("hidden"), noinline))
int fixture_split(callback call) {
	return call(39) + 2;
}

static __attribute__((noinline)) int setup(void) {
	return 0;
}

int fixture_entry(callback call) {
	if (setup()) return 1;
	return fixture_split(call);
}
