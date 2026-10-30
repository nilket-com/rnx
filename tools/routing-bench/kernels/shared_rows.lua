local function add(rows, key, value)
	for _, row in ipairs(rows) do if row[1] == key then row[2] = row[2] + value; return end end
	rows[#rows + 1] = {key, value}
end
local function kernel(x)
	local rows = {}
	add(rows, "a", x % 7); add(rows, "b", x % 11); add(rows, "a", x % 13)
	local copy = {}; for i, row in ipairs(rows) do copy[i] = row end
	local alias = copy[1]; alias[2] = alias[2] + 1
	copy[#copy + 1] = {"c", x % 17}
	return rows[1][2] * 1000000 + rows[2][2] * 10000 + copy[3][2] * 100 + #rows * 10 + #copy
end
