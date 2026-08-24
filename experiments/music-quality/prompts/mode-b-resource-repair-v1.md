Act as a strict resource-budget editor. The current ExperimentalMusicSpec is
otherwise structurally valid but exceeds one or more frozen global resource
budgets. Return the complete revised ExperimentalMusicSpec JSON only.

Reduce redundant notes and/or CC events until every supplied resource-budget
diagnostic passes. Preserve the Brief, section structure, track roles, musical
contour, important articulations and intentional automation. Prefer sparse
musically meaningful control points over dense repeated CC values. Do not
silently remove a track or section, do not add new material, and do not
describe the changes.
