#include "../engine_image.h"
#include <assert.h>
#include <pthread.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

typedef struct rnx_handle Handle;
typedef struct rnx_handles_request Request;
typedef struct rnx_response Response;
static const Handle none = {0};
static const char *script, *failing, *ordinary;
struct state {
	int mode, creates, reads, drops, offset;
	size_t retained, temporary, credit, live;
};

static Request request(unsigned op, Handle ctx, Handle child, Response *out) {
	return (Request){.version=2,.size=sizeof(Request),.operation=op,
		.context=ctx,.child=child,.output=out};
}
static void must(Request *r, int expected) {
	int result = rnx_engine_handles_v2(r);
	if (result != expected) {
		fprintf(stderr,"operation %u: got %d, expected %d\n",r->operation,result,expected);
	}
	assert(result == expected);
}
static int32_t create(void *, size_t, struct rnx_created *);
static int64_t read_item(void *, uint64_t);
static void release(void *, uint64_t);
static void reentry(void) {
	struct rnx_provider provider={NULL,create,read_item,release};

	Response out;
	Request r=request(RNX_OPEN,none,none,&out);
	r.provider=&provider;
	struct rnx_request old={.version=1,.size=sizeof(old),.path=(const uint8_t*)ordinary,
		.path_len=strlen(ordinary),.fuel=10000,.written=&out.written,.needed=&out.needed};
	assert(rnx_engine_run_file_v1(&old)==RNX_INVALID);
	must(&r,RNX_INVALID);
}
static int32_t create(void *p, size_t credit, struct rnx_created *out) {
	struct state *s=p;
	s->credit=credit;
	++s->creates;
	reentry();
	if (s->mode==1) return 1;
	assert(s->live==0 || s->mode==3);
	s->live+=s->retained;
	*out=(struct rnx_created){.token=(uint64_t)s->creates,.kind=s->mode==2 ? 2u : 1u,
		.retained=s->retained,.temporary=s->temporary};
	return 0;
}
static int64_t read_item(void *p,uint64_t id) {
	struct state *s=p;
	assert(id && id<=(uint64_t)s->creates && s->live>=s->retained);
	++s->reads;
	return 20+s->offset;
}
static void release(void *p,uint64_t id) {
	struct state *s=p;
	assert(id && id<=(uint64_t)s->creates && s->live>=s->retained);
	s->live-=s->retained;
	++s->drops;
	reentry();
}
static Handle open_context(struct state *s) {
	struct rnx_provider provider={s,create,read_item,release};
	Response out;
	Request r=request(RNX_OPEN,none,none,&out);
	r.provider=&provider;
	must(&r,RNX_OK);
	return out.handle; /* descriptor storage ends here */
}
static Handle compile(Handle ctx,const char *path,int status) {
	Response out;
	Request r=request(RNX_COMPILE,ctx,none,&out);
	r.path=(const uint8_t*)path;
	r.path_len=strlen(path);
	must(&r,status);
	return out.handle;
}
static Handle prepare(Handle ctx,Handle program,size_t bytes,int status) {
	Response out={0};
	Request r=request(RNX_PREPARE,ctx,program,&out);
	struct rnx_argument arg={(const uint8_t*)"input",5};
	r.argv=&arg; r.argc=1;
	r.bytes=bytes;
	must(&r,status);
	return out.handle;
}
static void action(unsigned op,Handle ctx,Handle child,int status) {
	Response out;
	Request r=request(op,ctx,child,&out);
	must(&r,status);
}
static void run(Handle ctx, const char *path, size_t bytes, int status, int64_t result) {
	Handle program=compile(ctx,path,RNX_OK);
	Handle inv=prepare(ctx,program,bytes,RNX_OK);
	Response out;
	uint8_t small[1], full[1024];
	Request r=request(RNX_DRIVE,ctx,inv,&out);
	r.fuel=100000;
	r.buffer=small;
	r.capacity=sizeof(small);
	must(&r,status);
	size_t needed=out.needed;
	assert(out.scalar==result && (status==RNX_OK || (needed>1 && out.written==1)));
	must(&r,RNX_INVALID);
	r.operation=RNX_REPORT;
	r.fuel=0;
	r.buffer=full;
	r.capacity=sizeof(full);
	must(&r,status);
	assert(out.scalar==result && out.needed==needed && out.written==needed);
	if (needed) assert(full[0]==small[0]);
	action(RNX_RELEASE_INVOCATION,ctx,inv,RNX_OK);
	action(RNX_RELEASE_INVOCATION,ctx,inv,RNX_INVALID);
}
static void *foreign_thread(void *arg) {
	Handle foreign=*(Handle*)arg;
	struct state s={.retained=32,.offset=3};
	Handle local=open_context(&s);
	action(RNX_CLOSE,foreign,none,RNX_INVALID);
	run(local,script,INTPTR_MAX,RNX_OK,46);
	action(RNX_CLOSE,local,none,RNX_OK);
	assert(s.drops==1 && s.live==0);
	return NULL;
}
int main(int argc,char **argv) {
	assert(argc==5);
	script=argv[1]; failing=argv[2]; ordinary=argv[3];
	struct state s={.retained=32,
#ifdef TAGGED
		.offset=2
#else
		.offset=1
#endif
	};
	Handle ctx=open_context(&s);
	compile(ctx,ordinary,RNX_REFUSED);
	Handle p=compile(ctx,script,RNX_OK);
	action(RNX_CLOSE,ctx,none,RNX_INVALID);
	action(RNX_RELEASE_INVOCATION,ctx,p,RNX_INVALID);
	Handle other=open_context(&s);
	action(RNX_DISCARD_PROGRAM,other,p,RNX_INVALID);
	action(RNX_CLOSE,other,none,RNX_OK);
	Response out;
	Request malformed=request(RNX_PREPARE,ctx,p,&out);
	malformed.flags=4;
	must(&malformed,RNX_INVALID);
	Handle inv=prepare(ctx,p,65536,RNX_OK);
	action(RNX_REPORT,ctx,inv,RNX_INVALID);
	action(RNX_RELEASE_INVOCATION,ctx,inv,RNX_OK);
	p=compile(ctx,script,RNX_OK);
	prepare(ctx,p,0,RNX_FAILED);
	prepare(ctx,p,65536,RNX_INVALID);
	assert(s.creates==0 && !s.drops);
	pthread_t thread;
	assert(!pthread_create(&thread,NULL,foreign_thread,&ctx));
	int64_t expected=2*(20+s.offset);
	run(ctx,script,65536,RNX_OK,expected);
	size_t credit=s.credit;
	assert(s.creates==1 && s.reads==2 && s.drops==1 && !s.live && credit<65536);
	s.temporary=credit-s.retained;
	run(ctx,script,65536,RNX_OK,expected);
	assert(s.creates==2 && s.reads==4 && s.drops==2 && !s.live);
	run(ctx,script,65535,RNX_FAILED,0);
	assert(s.credit==credit-1 && s.reads==4 && s.drops==3 && !s.live);
	s.temporary=SIZE_MAX;
	run(ctx,script,65536,RNX_FAILED,0);
	assert(s.drops==4 && !s.live);
	s.temporary=0;
	s.mode=1;
	run(ctx,script,65536,RNX_FAILED,0);
	assert(s.drops==4 && !s.live);
	s.mode=2;
	run(ctx,script,65536,RNX_FAILED,0);
	assert(s.drops==5 && !s.live);
	s.mode=0;
	run(ctx,failing,65536,RNX_FAILED,0);
	assert(s.drops==6 && !s.live);
	/* While the first payload is live the second create sees its retained
	 * bytes exactly once, regardless of the local alias or temporary charge. */
	s.mode=3;
	run(ctx,argv[4],65536,RNX_OK,3*(20+s.offset));
	size_t second=s.credit;
	s.retained+=256;
	s.temporary+=512;
	run(ctx,argv[4],65536,RNX_OK,3*(20+s.offset));
	assert(second==s.credit+256 && !s.live && s.drops==10);
	assert(!pthread_join(thread,NULL));

	action(RNX_CLOSE,ctx,none,RNX_OK);
	Handle next=open_context(&s);
	assert(ctx.slot==next.slot && ctx.generation!=next.generation);
	action(RNX_CLOSE,ctx,none,RNX_INVALID);
	action(RNX_CLOSE,next,none,RNX_OK);
	printf("checked scalar=%lld\n",(long long)expected);
}
