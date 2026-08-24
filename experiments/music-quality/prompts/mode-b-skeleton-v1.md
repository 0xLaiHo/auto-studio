Design the musical skeleton before writing notes. Output exactly this JSON shape:

{"title":"...","tempo_map":[...],"key_map":[...],"sections":[...],"track_plan":[{"id":"...","name":"...","role":"...","register":{"low":0,"high":127},"instrument_hint":"...","section_jobs":[{"section_id":"...","intent":"...","density":"sparse|medium|dense"}]}],"motif_strategy":"...","harmony_strategy":"...","groove_strategy":"..."}

Use the same field shapes for tempo_map, key_map, sections and register as ExperimentalMusicSpec. Do not include notes yet. Output only JSON.
