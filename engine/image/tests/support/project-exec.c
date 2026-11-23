// Observe our own test child at entry to execve. At the third exec the image
// has finished observing sources and blocked SIGINT, but legacy has not run.
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ptrace.h>
#include <sys/syscall.h>
#include <sys/user.h>
#include <sys/wait.h>
#include <unistd.h>

static void fail(const char *operation) {
	perror(operation);
	exit(1);
}

static void start(const char *app, const char *entry) {
	if (setenv("RNX_FILE_ROUTE_FD", "9", 1) || ptrace(PTRACE_TRACEME, 0, 0, 0) < 0)
		_exit(125);
	raise(SIGSTOP);
	execl(app, app, "run", "--budget", "1000000", entry, "with space", (char *)NULL);
	_exit(125);
}

static void change_inputs(const char *mode, pid_t child) {
	if (unlink("main.rn")) fail("unlink entry");
	if (!strcmp(mode, "Native")) {
		FILE *module = fopen("witness.rn", "w");
		if (!module || fputs("pub fn tag(){999}", module) < 0 || fclose(module))
			fail("change observed module");
	} else if (!strcmp(mode, "interrupt") && kill(child, SIGINT)) {
		fail("interrupt continuation");
	}
}

int main(int argc, char **argv) {
	if (argc != 4) return 2;
	const char *mode = argv[3];
	if (chdir(argv[2])) fail("chdir");
	char entry[PATH_MAX];
	if (!realpath("main.rn", entry)) fail("resolve entry");
	pid_t child = fork();
	if (child < 0) fail("fork");
	if (!child) start(argv[1], entry);

	int status, signal = 0, execs = 0, deliveries = 0;
	if (waitpid(child, &status, 0) != child || !WIFSTOPPED(status)) fail("initial stop");
	if (ptrace(PTRACE_SETOPTIONS, child, 0,
	           PTRACE_O_TRACESYSGOOD | PTRACE_O_TRACEEXEC | PTRACE_O_EXITKILL) < 0)
		fail("trace options");
	for (;;) {
		if (ptrace(PTRACE_SYSCALL, child, 0, signal) < 0 || waitpid(child, &status, 0) != child)
			fail("trace child");
		if (!WIFSTOPPED(status)) break;
		signal = WSTOPSIG(status);
		if (signal == (SIGTRAP | 0x80)) {
			struct user_regs_struct regs;
			if (ptrace(PTRACE_GETREGS, child, 0, &regs) < 0) fail("read registers");
			if (regs.orig_rax == SYS_execve && (long)regs.rax == -ENOSYS && ++execs == 3)
				change_inputs(mode, child);
			signal = 0;
		} else if (signal == SIGTRAP && (status >> 16) == PTRACE_EVENT_EXEC) {
			signal = 0;
		} else if (signal == SIGINT) {
			++deliveries;
		}
	}
	int code = WIFEXITED(status) ? WEXITSTATUS(status) : 128 + WTERMSIG(status);
	int interrupt = !strcmp(mode, "interrupt");
	if (execs == 3 && deliveries == interrupt && code == (interrupt ? 130 : 17)) return 0;
	fprintf(stderr, "pid=%ld execs=%d SIGINT deliveries=%d exit=%d\n",
	        (long)child, execs, deliveries, code);
	return 1;
}
