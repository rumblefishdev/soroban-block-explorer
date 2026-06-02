-- Pandoc Lua filter for the SCF Milestone 1 evidence PDF.
--
-- (1) Force every Table to use equal fractional column widths so it spans
--     the full page width. Pandoc-typst translates ColWidth=1/n into
--     `#table(columns: (Xfr, Yfr, ...), ...)`, which Typst stretches to
--     full page width. Without this, ColWidthDefault yields `columns: N`
--     (auto-fit), producing narrow tables with whitespace on the right.
--
-- (2) Center every standalone Image paragraph. Pandoc-typst emits
--     `#box(image(...))` for inline images, which is left-aligned in its
--     paragraph. We wrap the paragraph in a typst `#align(center)[...]`
--     RawBlock so figures sit centered on the page.

function Table(tbl)
  local n = #tbl.colspecs
  if n == 0 then return tbl end
  local w = 1 / n
  for i, cs in ipairs(tbl.colspecs) do
    -- cs is a {align, width} pair; preserve alignment, set width.
    tbl.colspecs[i] = { cs[1], w }
  end
  return tbl
end

-- A Para is "image-only" when its single child is an Image (the typical
-- pandoc representation of a markdown line that contains just `![…](…)`).
local function is_image_only_para(para)
  if #para.content ~= 1 then return false end
  return para.content[1].t == "Image"
end

function Para(para)
  if not is_image_only_para(para) then return nil end
  return {
    pandoc.RawBlock("typst", "#align(center)["),
    para,
    pandoc.RawBlock("typst", "]"),
  }
end
