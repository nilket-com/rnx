#include "../adapter_provider.h"
#include <assert.h>
#include <limits.h>
#include <string.h>

#define ENTRY(n,id,s,r,a,o) {{(const uint8_t *)(n),sizeof(n)-1},id,s,r,a,o,0}
static const struct rnx_declaration declarations[] = {
	ENTRY("read_csv",1,RNX_CSV,0,0,RNX_FRAME),
	ENTRY("width",2,RNX_INTEGER,RNX_FRAME,0,0),
	ENTRY("to_dense",3,RNX_COLUMNS,RNX_FRAME,0,RNX_DENSE),
	ENTRY("from_dense",4,RNX_CONVERT,0,RNX_DENSE,RNX_TENSOR),
	ENTRY("load",5,RNX_LOAD,0,0,RNX_MLP),
	ENTRY("forward",6,RNX_FORWARD,RNX_MLP,RNX_TENSOR,RNX_TENSOR),
	ENTRY("rank",7,RNX_INTEGER,RNX_TENSOR,0,0),
	ENTRY("elem_count",8,RNX_INTEGER,RNX_TENSOR,0,0),
	ENTRY("shape",9,RNX_PAIR,RNX_DENSE,0,0),
	ENTRY("dtype",10,RNX_TEXT,RNX_DENSE,0,0),
	ENTRY("names",11,RNX_NAMES,RNX_DENSE,0,0),
	ENTRY("dims",12,RNX_TRIPLE,RNX_MLP,0,0),
	ENTRY("display",13,RNX_DISPLAY,RNX_FRAME,0,0),
	ENTRY("display",14,RNX_DISPLAY,RNX_DENSE,0,0),
	ENTRY("display",15,RNX_DISPLAY,RNX_TENSOR,0,0)
};
struct counts { int creates, drops, reads, mode; };
static void put(void *p,uint64_t token) {
	struct counts *c=p;
	assert(token==42 && ++c->drops==1);
}
static void op(void *p,uint32_t id,const struct rnx_adapter_input *input,void *sink,rnx_emit emit) {
	struct counts *c=p;
	struct rnx_adapter_output output={.outcome=RNX_VALUE};
	if (id==5) {
		assert(++c->creates==1 && input->path.len==5 && !memcmp(input->path.data,"model",5));
		output.payload=(struct rnx_created){42,c->mode==1?RNX_TENSOR:RNX_MLP,c->mode==2?SIZE_MAX:c->mode==3?2097152:10,0};
	} else {
		assert(id==12 && input->receiver==42 && c->drops==0);
		++c->reads;
		output.integers[0]=6;
	}
	emit(sink,&output);
}
static void exercise(const char *path, int mode) {
	struct counts counts={.mode=mode};
	struct rnx_adapter_provider provider={&counts,declarations,sizeof(declarations)/sizeof(*declarations),op,put};
	struct rnx_response out={0};
	uint8_t buf[1024]={0};
	struct rnx_adapter_request r={.version=3,.size=sizeof(r),.operation=RNX_OPEN,.output=&out,.buffer=buf,.capacity=sizeof(buf)};
	assert(rnx_engine_adapters_v3(&r)==RNX_INVALID);
	r.provider=&provider;
	--provider.count;
	assert(rnx_engine_adapters_v3(&r)==RNX_REFUSED);
	++provider.count;
	assert(rnx_engine_adapters_v3(&r)==RNX_OK);
	r.context=out.handle;
	r.provider=NULL;
	r.operation=RNX_COMPILE;
	r.path=(const uint8_t *)path;
	r.path_len=strlen(path);
	assert(rnx_engine_adapters_v3(&r)==RNX_OK);
	struct rnx_handle program=out.handle;
	struct rnx_adapter_request other={.version=3,.size=sizeof(r),.operation=RNX_OPEN,.provider=&provider,.output=&out};
	assert(rnx_engine_adapters_v3(&other)==RNX_OK);
	other.context=out.handle;
	other.operation=RNX_PREPARE;
	other.child=program;
	other.provider=NULL;
	assert(rnx_engine_adapters_v3(&other)==RNX_INVALID);
	other.operation=RNX_CLOSE;
	other.child=(struct rnx_handle){0};
	assert(rnx_engine_adapters_v3(&other)==RNX_OK);
	r.path=NULL;r.path_len=0;r.child=program;r.operation=RNX_PREPARE;r.bytes=1048576;
	assert(rnx_engine_adapters_v3(&r)==RNX_OK);
	r.child=out.handle;r.operation=RNX_DRIVE;r.bytes=0;r.fuel=100000;r.capacity=1;
	int status=rnx_engine_adapters_v3(&r);
	assert(status==(mode==0?RNX_OK:RNX_FAILED));
	assert(counts.creates==1 && counts.reads==(mode==0?2:0));
	size_t needed=out.needed;
	if (mode) assert(needed>1 && out.written==1);
	r.operation=RNX_REPORT;r.fuel=0;r.capacity=sizeof(buf);
	assert(rnx_engine_adapters_v3(&r)==status && out.needed==needed && out.written==needed);
	assert(counts.creates==1 && counts.reads==(mode==0?2:0));
	r.operation=RNX_RELEASE_INVOCATION;
	assert(rnx_engine_adapters_v3(&r)==RNX_OK && counts.drops==1);
	r.operation=RNX_CLOSE;r.child=(struct rnx_handle){0};
	assert(rnx_engine_adapters_v3(&r)==RNX_OK);
}
int main(int argc,char **argv) {
	assert(argc==2);
	for (int mode=0;mode<4;++mode) exercise(argv[1],mode);
	return 0;
}
