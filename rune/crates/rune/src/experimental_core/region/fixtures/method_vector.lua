local Accumulator = {}; Accumulator.__index = Accumulator
function Accumulator:bump(index)
	local value = self.values[index+1] + 1
	self.values[index+1] = value
	return value
end
function Accumulator:visit(n)
	if n > 0 then
		local value = self:bump(n%4)
		self.total = self.total + value
		self:visit(n-1)
	end
end
local function kernel(seed)
	local accumulator = setmetatable({total=0,values={1,2,3,4}},Accumulator)
	accumulator:visit(100+seed%7)
	return accumulator.total
end
