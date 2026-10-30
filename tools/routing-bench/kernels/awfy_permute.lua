-- AWFY Permute; see ../NOTICE.txt and ../AWFY-SOURCES.json.
local Permute = {}; Permute.__index = Permute
function Permute:swap(i, j)
	local tmp = self.v[i]; self.v[i] = self.v[j]; self.v[j] = tmp
end
function Permute:permute(n)
	self.count = self.count + 1
	if n ~= 0 then
		local n1 = n - 1; self:permute(n1); local i = n1
		while i >= 0 do
			self:swap(n1, i); self:permute(n1); self:swap(n1, i); i = i - 1
		end
	end
end
function Permute:benchmark()
	self.count = 0; self.v = {[0]=0, 0, 0, 0, 0, 0}; self:permute(6); return self.count
end
function Permute:verify_result(result) return result == 8660 end
local function kernel(x)
	local b = setmetatable({count=0, v={}}, Permute); local result = b:benchmark()
	assert(b:verify_result(result)); return result
end
