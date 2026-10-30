local function step(x, y) return (x * 17 + y * 31) % 1000003 end
local function kernel(x) for i = 0, 9999 do x = step(x, i) end return x end
