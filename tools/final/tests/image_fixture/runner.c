extern int fixture_entry(int (*call)(int));
extern int in_runner(int);

__asm__(".text\n.globl in_runner\n.type in_runner,@function\n"
        "in_runner:\nnop\nlea 1(%rdi),%eax\nret\n.size in_runner,.-in_runner");

int main(void) {
	return fixture_entry(in_runner) == 42 ? 0 : 1;
}
