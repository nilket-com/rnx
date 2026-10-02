# Upstream defect draft (huggingface/candle): CPU `conv1d` ignores its contiguous kernel copy

Status: a draft for the operator, **not filed.** Found by record 0145 while bounding Candle's signal operations, confirmed in the pinned source, and reproduced by `adapters/candle/tests/signal.rs` (`views_give_the_logical_result_and_the_upstream_defect_is_shown`).

**Where:** candle-core 0.11.0, `src/cpu_backend/mod.rs`, `impl BackendStorage for CpuStorage`, `fn conv1d` (the im2col branch, around line 2785):

```rust
} else {
    // Make the kernel contiguous if not already the case.
    let mut kernel_c = unsafe {
        self.device()
            .alloc_uninit(kernel_l.shape(), kernel.dtype())?
    };
    kernel.copy_strided_src(&mut kernel_c, 0, kernel_l)?;
    let kernel_l = Layout::contiguous_with_offset((1, n, k), kernel_l.start_offset())
        .transpose(1, 2)?
        .broadcast_as((b, k, n))?;
    col.matmul(kernel, (b, m, n, k), &col_l, &kernel_l)?   // <- `kernel`, not `kernel_c`
};
```

**The defect:** the non-contiguous branch copies the kernel into `kernel_c`, then multiplies by the **original** `kernel`, read through an imposed contiguous layout (also at the original's start offset). A strided or transposed kernel is therefore read in the wrong element order, and the result is silently wrong. `kernel_c` is allocated and filled for nothing.

**Reproduction** (CPU, candle-core 0.11.0, default features):

```rust
use candle_core::{DType, Device, Tensor};
let cpu = &Device::Cpu;
let x = Tensor::arange(0f32, 2.0 * 4.0 * 17.0, cpu)?.reshape((2, 4, 17))?;
let raw = Tensor::arange(0f32, 3.0 * 4.0 * 8.0, cpu)?.reshape((3, 4, 8))?;
let k = raw.permute((2, 1, 0))?;              // [8, 4, 3], not contiguous
let logical = x.conv1d(&k.contiguous()?, 1, 1, 1, 1)?;
let direct = x.conv1d(&k, 1, 1, 1, 1)?;
assert_ne!(direct.to_vec3::<f32>()?, logical.to_vec3::<f32>()?);   // differs
```

**The fix:** pass `&kernel_c` (with its own offset-zero contiguous layout) to `matmul`, which is what the comment says the branch does.

**How rnx is affected:** it isn't, any more. rnx's Candle adapter canonicalizes every kernel and input to fresh, contiguous, offset-zero storage before any signal call (record 0145), and its controls compare views against the logical result.

## Related hazards found by records 0143 and 0145 (candidates, not defects in the same sense)

- **`npy.rs`:** `read_header` allocates the declared header length (up to 4 GiB in format 2) before reading it, and `from_reader` allocates the declared element count before reading any data. A few-byte file can request a huge allocation.
- **`conv.rs`:** the convolution output sizes use unchecked `usize` arithmetic. A kernel larger than the padded input underflows (a debug panic, a release wrap), and a zero stride divides by zero. `ParamsConvTranspose1D::l_out` subtracts `2·padding` before adding the dilated kernel, so it underflows for some shapes whose true output is positive.
- **`scatter`:** an index equal to the dtype's maximum is skipped silently rather than refused.
- **`upsample_bilinear2d_with_scale`:** computes `floor(h · scale) as usize`, which saturates a NaN scale to 0 and an infinity to `usize::MAX`.
