local Counter = {}; Counter.__index = Counter
function Counter:add(value)
	if self.enabled then self.total = self.total + value end
	self.enabled = not self.enabled
end
function Counter:run(n)
	local i = 0
	while i < n do self:add(i); i = i + 1 end
	return self.total
end
local function kernel(seed)
	local counter = setmetatable({total=0,enabled=true},Counter)
	return counter:run(200+seed%7)
end
