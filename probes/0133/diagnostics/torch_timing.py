"""Record 0133, diagnostic: PyTorch CPU on the same D1 passages, for context
(sentence-transformers, batch size 32).

  torch_timing.py MODEL_DIR DATA_DIR    (needs torch and sentence-transformers)"""
import pathlib, resource, sys, time
sys.path.insert(0, str(pathlib.Path(__file__).parent))
import chunking, sentence_transformers, torch

model, data = sys.argv[1:3]
passages = [c for _, _, cs in chunking.load(f"{data}/d1/plans") for c in cs]
m = sentence_transformers.SentenceTransformer(model, device="cpu")
t = time.perf_counter()
m.encode(passages, batch_size=32)
print(f"{len(passages)} passages: PyTorch {torch.__version__} CPU ({torch.get_num_threads()} threads) {time.perf_counter() - t:.1f} s, "
      f"peak RSS {resource.getrusage(resource.RUSAGE_SELF).ru_maxrss // 1024} MiB")
