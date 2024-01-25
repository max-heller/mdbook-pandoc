-- needs pandoc 2.9.2
function Blocks (blocks)
  for i = #blocks, 1, -1 do
    if i < #blocks then
      -- check if current block is a `mdbook-pandoc::table: width1|width2|width3` annotation
      if blocks[i].t == 'RawBlock' and blocks[i].format == 'html' and blocks[i+1].t == 'Table' then
        local html = blocks[i].text
        local table = blocks[i+1]
        local _,end_idx = html:find('mdbook%-pandoc::table: ')
        if end_idx ~= nil then
          local widths = {}
          local total_width = 0
          for width in html:sub(end_idx+1):gmatch('(%d[^| ]*)') do
            local width_n = tonumber(width)
            widths[#widths+1] = width_n
            total_width = total_width + width_n
          end
          for j,col in pairs(table.colspecs) do
          	col[2] = widths[j] / total_width
          end
          -- remove annotation
        	blocks:remove(i)
        end
      end
    end
  end
  return blocks
end
