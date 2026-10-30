local function kernel(x)
	local total = x
	for i = 0, 9999 do total = (total + i % 97) % 1000000007 end
	return total
end
