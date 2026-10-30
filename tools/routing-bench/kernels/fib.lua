local function fib(n) if n < 2 then return n else return fib(n - 1) + fib(n - 2) end end
local function kernel(x) return fib(20 + x % 2) end
