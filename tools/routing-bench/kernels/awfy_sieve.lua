-- AWFY Sieve; see ../NOTICE.txt and ../AWFY-SOURCES.json.
local Sieve = {}; Sieve.__index = Sieve
function Sieve:sieve(flags, size)
	local count = 0
	for i = 2, size do
		if flags[i - 2] then
			count = count + 1; local k = i + i
			while k <= size do flags[k - 2] = false; k = k + i end
		end
	end
	return count
end
function Sieve:benchmark()
	local flags = {}; for i = 0, 4999 do flags[i] = true end
	return self:sieve(flags, 5000)
end
function Sieve:verify_result(result) return result == 669 end
local function kernel(x)
	local b = setmetatable({}, Sieve); local result = b:benchmark()
	assert(b:verify_result(result)); return result
end
