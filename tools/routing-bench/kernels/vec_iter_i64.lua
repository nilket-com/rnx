local function sum(values)
	local total=0
	for _,value in ipairs(values) do total=total+value end
	return total
end
local function kernel(seed)
	local values={}
	for i=0,9999 do values[#values+1]=(i+seed)%97 end
	local total=0
	for repeat_index=1,8 do total=(total+sum(values))%1000000007 end
	return total
end
