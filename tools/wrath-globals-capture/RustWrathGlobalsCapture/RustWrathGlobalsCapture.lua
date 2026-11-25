-- Fills RustWrathGlobalsCapture with "name<TAB>type" rows at login. The file the client
-- writes at logout/reload is what tools/import-globals-capture.ps1 turns into the 3.3.5a
-- surface oracle. No file I/O: the SavedVariables writer is the only way out of the sandbox.
RustWrathGlobalsCapture = RustWrathGlobalsCapture or {}
RustWrathGlobalsCaptureBuild = RustWrathGlobalsCaptureBuild or {}

local capture = CreateFrame("Frame")
capture:RegisterEvent("PLAYER_LOGIN")
capture:SetScript("OnEvent", function()
    local rows = {}
    for name, value in pairs(_G) do
        if type(name) == "string" then
            rows[#rows + 1] = name .. "\t" .. type(value)
        end
    end
    table.sort(rows)
    RustWrathGlobalsCapture = rows
    RustWrathGlobalsCaptureBuild = { GetBuildInfo() }
    print("RustWrathGlobalsCapture: " .. #rows .. " names; /reload or log out to write the SavedVariables file")
end)
