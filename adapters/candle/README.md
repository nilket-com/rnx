# rnx-candle

Record 0129: CPU tensors and one small fixed model for rnx, scoped to a single
cross-adapter workflow (Polars data → tensor → inference → predictions back
into a frame). Numbers cross between adapters only through rnx's neutral
`interchange::Dense` block; this crate does not depend on any other adapter.

- `candle::Tensor::from_dense(block)`, `tensor.to_dense(names)`, `shape()`,
  `dtype()`, and a bounded display (dtype, shape, at most 8 × 8 values, at most
  2,048 bytes). In a notebook the same corner also comes as an HTML table with row
  and column indices and `…` where values were left out (record 0161): built from
  the same bounded read, escaped, with no attribute, class, style or script.
- `candle::Mlp::load(path)`: exactly `fc1.weight [hidden, in]`, `fc1.bias
  [hidden]`, `fc2.weight [out, hidden]`, `fc2.bias [out]`, all F32, from a local
  safetensors file of at most 64 MiB (bounded read, never memory-mapped).
- `mlp.forward(tensor)`: `Linear`, ReLU, `Linear` on the CPU; a 2-D F32 input.

CPU only: Candle 0.11.0's default features are empty (no CUDA, Metal, MKL or
Accelerate), and no non-Rust system library is linked. Candle is MIT OR
Apache-2.0.
