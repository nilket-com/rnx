#include "../../../engine/image/adapter_provider.h"
#include <stdio.h>
/* Canonicalize the actual table passed to OPEN for the equality control. */
int rnx_dump_file_inventory(const struct rnx_adapter_provider *p) {
	static const char *const kinds[]={"", "polars::DataFrame", "interchange::Dense","candle::Tensor","candle::Mlp","polars::Error"};
	for (size_t i=0;i<p->count;i++) {
		const struct rnx_declaration *d=p->declarations+i;
		if (d->receiver>=sizeof(kinds)/sizeof(*kinds) || d->result>=sizeof(kinds)/sizeof(*kinds)) return 1;
		if (d->id) {
			const char *owner=d->shape==RNX_CSV || d->shape==RNX_CSV_PRE_EFFECT_REFUSAL ? "polars"
				: kinds[d->receiver ? d->receiver : d->result];
			printf("%s::",owner);
		}
		if (d->shape==RNX_DISPLAY) fputs("__opaque_display",stdout);
		else fwrite(d->name.data,1,d->name.len,stdout);
		printf("\t%u\t%u\t%u\t%u\n",d->shape,d->receiver,d->argument,d->result);
	}
	return ferror(stdout)!=0;
}
