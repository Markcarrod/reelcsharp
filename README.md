# Reel Forge C#

Native C# WinForms reel generator. It parses `.txt` reel scripts, creates 1080x1920 transparent text overlays, and calls FFmpeg to compose vertical MP4 reels with optional background videos, music, blur/tint treatments, batch folders, logs, and saved desktop settings.

## Requirements

- .NET SDK 8 or newer
- FFmpeg and FFprobe available on `PATH`
- Windows, because this app uses WinForms

## Run

```powershell
dotnet run --project .\ReelForgeCSharp.csproj
```

CLI mode is also supported:

```powershell
dotnet run --project .\ReelForgeCSharp.csproj -- --script .\input\scripts\example.txt --videos .\input\videos --music .\input\music --output .\output\videos --overlays .\output\overlays --workers auto --blur none
```

Saved desktop settings live at `config/desktop_state.json`.
