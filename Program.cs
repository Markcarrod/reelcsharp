using System.Collections.Concurrent;
using System.Diagnostics;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Drawing.Imaging;
using System.Globalization;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;
using System.Text.RegularExpressions;
#if WINDOWS
using System.Windows.Forms;
#endif

namespace ReelForgeCSharp;

internal static class Program
{
    [STAThread]
    private static int Main(string[] args)
    {
        try
        {
            if (args.Any(a => a.Equals("--script", StringComparison.OrdinalIgnoreCase)))
            {
                try
                {
                    try
                    {
                        Console.OutputEncoding = Encoding.UTF8;
                    }
                    catch (IOException)
                    {
                        // Some hosts do not expose a writable console handle.
                    }

                    return Cli.Run(args);
                }
                catch (Exception ex)
                {
                    Console.Error.WriteLine(ex.Message);
                    return 1;
                }
            }

#if WINDOWS
            ApplicationConfiguration.Initialize();
            Application.Run(new ReelForgeForm());
            return 0;
#else
            Console.Error.WriteLine("ReelForge desktop UI is only available on Windows. Use CLI mode with --script on Linux.");
            return 1;
#endif
        }
        catch (Exception ex)
        {
            var logPath = Path.Combine(Directory.GetCurrentDirectory(), "startup_crash.log");
            File.WriteAllText(logPath, ex.ToString(), Encoding.UTF8);
#if WINDOWS
            MessageBox.Show(
                $"Reel Forge C# failed to start.\n\nA crash log was written to:\n{logPath}\n\n{ex.Message}",
                "Reel Forge C#",
                MessageBoxButtons.OK,
                MessageBoxIcon.Error);
#else
            Console.Error.WriteLine($"Reel Forge C# failed to start. A crash log was written to: {logPath}");
            Console.Error.WriteLine(ex.Message);
#endif
            return 1;
        }
    }
}

internal static class AppPaths
{
    public static readonly string Root = Directory.GetCurrentDirectory();
    public static readonly string StatePath = Path.Combine(Root, "config", "desktop_state.json");
    public static readonly string TempScriptPath = Path.Combine(Root, "output", "temp_script.txt");
    public static readonly string CompletionLedgerPath = @"C:\Users\Administrator\OneDrive\Videos\fbfINAL\ALL.TXT";
}

internal enum BlurStrength
{
    None,
    Light,
    Middle,
    Heavy
}

internal static class BlurStrengthExtensions
{
    public static string ToArg(this BlurStrength value) =>
        value switch
        {
            BlurStrength.Light => "light",
            BlurStrength.Middle => "middle",
            BlurStrength.Heavy => "heavy",
            _ => "none"
        };

    public static BlurStrength Parse(string? value) =>
        (value ?? "").Trim().ToLowerInvariant() switch
        {
            "light" => BlurStrength.Light,
            "middle" or "medium" => BlurStrength.Middle,
            "heavy" => BlurStrength.Heavy,
            _ => BlurStrength.None
        };
}

internal sealed class ReelScript
{
    public string Title { get; set; } = "";
    public List<string> Points { get; } = [];
    public List<int> PointPauseCountsBefore { get; } = [];
    public int CtaPauseCountBefore { get; set; }
    public string Cta { get; set; } = "";
    public string Code { get; set; } = "";
    public string Niche { get; set; } = "";
    public float? Duration { get; set; }
    public string Layout { get; set; } = "center_stack";
    public bool AllAtOnce { get; set; }
    public string? Video { get; set; }
    public string? Audio { get; set; }

}

internal static partial class ScriptParser
{
    private static readonly string[] PipeRowLayoutRotation =
    [
        "center_stack",
        "left_stack",
        "right_stack",
        "top_bottom",
        "one_word_hook",
        "quote_style",
        "story_block",
        "progress_reveal",
        "center_card",
        "two_column_split",
        "grid_layout",
        "masonry_layout",
        "hero_list",
        "alternating_rows",
        "sidebar_layout",
        "collage_layout",
        "auto_fit_tiles",
        "tabbed_layout",
        "magazine_layout",
        "template_rotation_layout",
        "priority_based_layout",
        "adaptive_smart_layout",
        "fallback_universal_layout"
    ];

    public static List<ReelScript> ParseFile(string path)
    {
        var content = File.ReadAllText(path, Encoding.UTF8);
        if (LooksLikePipeRows(content))
        {
            return ParsePipeRows(content);
        }

        var blocks = Regex.Split(content, @"(?m)^\s*---+\s*$")
            .Select(b => b.Trim())
            .Where(b => b.Length > 0)
            .ToList();

        var scripts = new List<ReelScript>();
        for (var i = 0; i < blocks.Count; i++)
        {
            scripts.Add(ParseBlock(blocks[i], i));
        }

        return scripts;
    }

    private static bool LooksLikePipeRows(string content)
    {
        var rows = content.Split(["\r\n", "\n"], StringSplitOptions.None)
            .Select(line => line.Trim().TrimStart('\ufeff').Trim())
            .Where(line => line.Length > 0 && !Regex.IsMatch(line, @"^\s*---+\s*$"))
            .ToList();

        return rows.Count > 0 && rows.All(IsPipeRow);
    }

    private static bool IsPipeRow(string line)
    {
        var cells = line.Split('|');
        return cells.Length >= 5 && cells.Count(cell => cell.Trim().Length > 0) >= 4;
    }

    private static List<ReelScript> ParsePipeRows(string content)
    {
        var scripts = new List<ReelScript>();
        var rows = content.Split(["\r\n", "\n"], StringSplitOptions.None)
            .Select(line => line.Trim().TrimStart('\ufeff').Trim())
            .Where(line => line.Length > 0 && !Regex.IsMatch(line, @"^\s*---+\s*$"));

        foreach (var row in rows)
        {
            if (!IsPipeRow(row))
            {
                continue;
            }

            scripts.Add(ParsePipeRow(row, scripts.Count));
        }

        return scripts;
    }

    private static ReelScript ParsePipeRow(string row, int index)
    {
        var cells = row.Split('|').Select(cell => cell.Trim()).ToList();
        var script = new ReelScript
        {
            Niche = cells.Count > 0 ? cells[0] : "",
            Title = cells.Count > 1 && cells[1].Length > 0 ? cells[1] : $"Video {index + 1}",
            Layout = PipeRowLayoutRotation[index % PipeRowLayoutRotation.Length]
        };

        var durationIndex = cells.FindIndex(cell => cell.StartsWith("Duration:", StringComparison.OrdinalIgnoreCase));
        var pointEnd = durationIndex >= 0 ? durationIndex : cells.Count;
        for (var i = 2; i < pointEnd; i++)
        {
            if (cells[i].Length == 0)
            {
                continue;
            }

            script.Points.Add(cells[i]);
            script.PointPauseCountsBefore.Add(0);
        }

        if (durationIndex >= 0)
        {
            var match = Regex.Match(cells[durationIndex], @"\d+(?:\.\d+)?");
            if (match.Success && float.TryParse(match.Value, NumberStyles.Float, CultureInfo.InvariantCulture, out var duration))
            {
                script.Duration = duration;
            }

            foreach (var cell in cells.Skip(durationIndex + 1).Where(cell => cell.Length > 0))
            {
                if (TryApplyPipeMetadata(script, cell))
                {
                    continue;
                }

                if (string.IsNullOrWhiteSpace(script.Code))
                {
                    script.Code = cell;
                }
            }
        }
        else
        {
            foreach (var cell in cells.Skip(2).Where(cell => cell.Length > 0))
            {
                TryApplyPipeMetadata(script, cell);
            }

            var code = cells.LastOrDefault(cell => cell.Length > 0 && !IsPipeMetadata(cell));
            if (!string.IsNullOrWhiteSpace(code) && !string.Equals(code, script.Title, StringComparison.OrdinalIgnoreCase))
            {
                script.Code = code;
            }
        }

        return script;
    }

    private static bool TryApplyPipeMetadata(ReelScript script, string cell)
    {
        var match = Regex.Match(cell, @"^\s*(audio|video|vid)\s*:\s*(.+?)\s*$", RegexOptions.IgnoreCase);
        if (!match.Success)
        {
            return false;
        }

        var value = NormalizeInputPath(match.Groups[2].Value.Trim());
        if (match.Groups[1].Value.Equals("audio", StringComparison.OrdinalIgnoreCase))
        {
            script.Audio = value.Length == 0 ? null : value;
        }
        else
        {
            script.Video = value.Length == 0 ? null : value;
        }

        return true;
    }

    private static bool IsPipeMetadata(string cell) =>
        Regex.IsMatch(cell, @"^\s*(audio|video|vid)\s*:", RegexOptions.IgnoreCase);

    private static string NormalizeInputPath(string path)
    {
        var trimmed = path.Trim().Trim('"');
        if (Regex.IsMatch(trimmed, @"^/home/kayan/", RegexOptions.IgnoreCase))
        {
            return Path.Combine(@"C:\Users\kayan", trimmed["/home/kayan/".Length..].Replace('/', Path.DirectorySeparatorChar));
        }

        var mntMatch = Regex.Match(trimmed, @"^/mnt/([a-z])/(.+)$", RegexOptions.IgnoreCase);
        if (mntMatch.Success)
        {
            return $"{mntMatch.Groups[1].Value.ToUpperInvariant()}:\\{mntMatch.Groups[2].Value.Replace('/', Path.DirectorySeparatorChar)}";
        }

        return trimmed;
    }

    public static string Slugify(string value)
    {
        var slug = Regex.Replace(value, @"[^a-zA-Z0-9]+", "-").Trim('-').ToLowerInvariant();
        return slug.Length == 0 ? "reel" : slug;
    }

    public static string NormalizeLayout(string value)
    {
        var normalized = Regex.Replace(value.ToLowerInvariant(), @"[^a-z0-9]+", "_").Trim('_');
        return normalized switch
        {
            "question_answer" or "question" or "answer" => "question_answer",
            "advice" or "list" or "list_style" => "list_style",
            "reels" or "center" or "center_stack" => "center_stack",
            "left" or "left_stack" => "left_stack",
            "right" or "right_stack" => "right_stack",
            "top_bottom" => "top_bottom",
            "one_word" or "one_word_hook" => "one_word_hook",
            "quote" or "quote_style" => "quote_style",
            "story" or "story_block" => "story_block",
            "full_text" or "full_list" or "all_at_once" or "static_text" => "story_block",
            "progress" or "progress_reveal" => "progress_reveal",
            "card" or "center_card" => "center_card",
            "two_column_split" => "two_column_split",
            "grid_layout" => "grid_layout",
            "masonry_layout" => "masonry_layout",
            "hero_list" => "hero_list",
            "alternating_rows" => "alternating_rows",
            "sidebar_layout" => "sidebar_layout",
            "collage_layout" => "collage_layout",
            "auto_fit_tiles" => "auto_fit_tiles",
            "tabbed_layout" => "tabbed_layout",
            "magazine_layout" => "magazine_layout",
            "template_rotation_layout" => "template_rotation_layout",
            "priority_based_layout" => "priority_based_layout",
            "adaptive_smart_layout" => "adaptive_smart_layout",
            "fallback_universal_layout" => "fallback_universal_layout",
            _ => "center_stack"
        };
    }

    public static ReelScript CollapseDuplicateTitlePoint(ReelScript script)
    {
        if (script.Points.Count == 0 || LooseTextKey(script.Title) != LooseTextKey(script.Points[0]))
        {
            return script;
        }

        var copy = new ReelScript
        {
            Title = script.Title,
            Cta = script.Cta,
            Code = script.Code,
            Niche = script.Niche,
            Duration = script.Duration,
            Layout = script.Layout,
            AllAtOnce = script.AllAtOnce,
            Video = script.Video,
            Audio = script.Audio,
            CtaPauseCountBefore = script.CtaPauseCountBefore
        };
        copy.Points.AddRange(script.Points.Skip(1));
        copy.PointPauseCountsBefore.AddRange(script.PointPauseCountsBefore.Skip(1));
        return copy;
    }

    private static ReelScript ParseBlock(string block, int index)
    {
        var script = new ReelScript();
        var pendingPauseCount = 0;
        var lineNumberRegex = new Regex(@"(?i)^line_?\d+$");
        var metadataKeyRegex = new Regex(@"(?i)^[a-z][a-z0-9_ -]*$");
        var listPrefixRegex = new Regex(@"^\s*(?:[-*]|\d+[.)])\s*");
        var durationRegex = new Regex(@"\d+(?:\.\d+)?");

        void PushPoint(string rawValue)
        {
            var cleaned = listPrefixRegex.Replace(rawValue, "").Trim();
            if (cleaned.Equals("(pause)", StringComparison.OrdinalIgnoreCase) ||
                cleaned.Equals("[pause]", StringComparison.OrdinalIgnoreCase))
            {
                pendingPauseCount++;
                return;
            }

            if (cleaned.Length == 0)
            {
                return;
            }

            script.Points.Add(cleaned);
            script.PointPauseCountsBefore.Add(pendingPauseCount);
            pendingPauseCount = 0;
        }

        foreach (var raw in block.Split(["\r\n", "\n"], StringSplitOptions.None))
        {
            var line = raw.Trim().TrimStart('\ufeff').Trim();
            if (line.Length == 0)
            {
                continue;
            }

            var colon = line.IndexOf(':');
            if (colon >= 0)
            {
                var key = line[..colon].Trim().ToLowerInvariant();
                var value = line[(colon + 1)..].Trim();
                switch (key)
                {
                    case "title":
                    case "question":
                        script.Title = value;
                        break;
                    case "cta":
                    case "caption":
                    case "fb_caption":
                        script.Cta = value;
                        break;
                    case "code":
                        script.Code = value;
                        break;
                    case "video":
                    case "vid":
                        script.Video = value.Length == 0 ? null : value;
                        break;
                    case "audio":
                        script.Audio = value.Length == 0 ? null : value;
                        break;
                    case "duration":
                        var match = durationRegex.Match(value);
                        if (match.Success && float.TryParse(match.Value, out var duration))
                        {
                            script.Duration = duration;
                        }
                        break;
                    case "format":
                    case "layout":
                        script.Layout = NormalizeLayout(value);
                        var layoutLower = value.ToLowerInvariant();
                        script.AllAtOnce = layoutLower.Contains("full") || layoutLower.Contains("static") || layoutLower.Contains("all at once");
                        break;
                    case "text_animation":
                        var animationLower = value.ToLowerInvariant();
                        if (animationLower.Contains("static") || animationLower.Contains("all at once") ||
                            animationLower.Contains("no pop") || animationLower.Contains("no popping") ||
                            animationLower.Contains("none"))
                        {
                            script.AllAtOnce = true;
                        }
                        else if (animationLower.Contains("question") && animationLower.Contains("answer"))
                        {
                            script.Layout = "question_answer";
                        }
                        else if (animationLower.Contains("fade") || animationLower.Contains("line"))
                        {
                            script.Layout = "list_style";
                        }
                        break;
                    case "style":
                    case "niche":
                    case "sub_style":
                        script.Niche = value;
                        break;
                    default:
                        if (lineNumberRegex.IsMatch(key))
                        {
                            PushPoint(value);
                        }
                        else if (!metadataKeyRegex.IsMatch(key))
                        {
                            PushPoint(line);
                        }
                        break;
                }
            }
            else
            {
                PushPoint(line);
            }
        }

        if (script.Title.Length == 0 && script.Points.Count > 0)
        {
            script.Title = script.Points[0];
            script.Points.RemoveAt(0);
            script.PointPauseCountsBefore.RemoveAt(0);
        }

        if (script.Title.Length == 0)
        {
            script.Title = $"Video {index + 1}";
        }

        script.CtaPauseCountBefore = pendingPauseCount;
        return script;
    }

    private static string LooseTextKey(string value) =>
        string.Join(" ", value.Select(ch => char.IsLetterOrDigit(ch) ? char.ToLowerInvariant(ch) : ' ')
            .Aggregate(new StringBuilder(), (sb, ch) => sb.Append(ch), sb => sb.ToString())
            .Split(' ', StringSplitOptions.RemoveEmptyEntries));
}

internal sealed record LayoutParam(float X, float Y, float Width, string Align, float FontSize);
internal sealed record LayoutSpec(LayoutParam Title, LayoutParam Point, LayoutParam Cta, string Marker);

internal static class OverlayRenderer
{
    public const int Width = 1080;
    public const int Height = 1920;
    private const float SafeTop = Height * 0.08f;
    private const float SafeBottom = Height * 0.78f;
    private const float BodyTop = Height * 0.22f;
    private const float BodyWidth = Width * 0.74f;
    private const float BodyX = (Width - BodyWidth) / 2f;
    private enum ReadableVariant
    {
        CenterStack,
        LeftStory,
        Focus,
        Card,
        Timeline,
        VerticalIndicator,
        Divider,
        Spotlight,
        MinimalFloating
    }
    private sealed record ReadableLayout(ReadableVariant Variant, float TitleY, float BodyY, float BodyWidth, string BodyAlign, float LineGapScale, float ParagraphGapScale);
    private static readonly Color[] CurrentLineAccentColors =
    [
        Color.FromArgb(255, 255, 217, 106), // warm yellow
        Color.FromArgb(255, 139, 229, 255), // bright cyan
        Color.FromArgb(255, 151, 255, 199), // mint
        Color.FromArgb(255, 255, 184, 128), // peach
        Color.FromArgb(255, 255, 151, 203), // soft pink
        Color.FromArgb(255, 204, 190, 255)  // light lavender
    ];

    public static string MakeOverlay(ReelScript script, int layerIndex, string outputFolder, string stamp)
    {
        Directory.CreateDirectory(outputFolder);
        var layout = ScriptParser.NormalizeLayout(script.Layout);
        var spec = GetLayoutSpec(layout);
        using var image = new Bitmap(Width, Height, PixelFormat.Format32bppArgb);
        using var g = Graphics.FromImage(image);
        g.Clear(Color.Transparent);
        g.SmoothingMode = SmoothingMode.AntiAlias;
        g.TextRenderingHint = System.Drawing.Text.TextRenderingHint.AntiAliasGridFit;

        if (!script.AllAtOnce)
        {
            DrawPopInStayOverlay(g, script, layout, layerIndex);
        }
        else if (layerIndex == 0)
        {
            var text = layout == "one_word_hook" ? FirstHookWord(script.Title) : NormalizeRenderText(script.Title);
            if (layout == "quote_style")
            {
                text = $"\"{text.Trim('"')}\"";
            }

            DrawWrapped(g, text, spec.Title, Color.White, Color.FromArgb(180, 0, 0, 0));
        }
        else
        {
            var pointIndex = layerIndex - 1;
            if (pointIndex < script.Points.Count)
            {
                var text = PointText(script, spec, layout, pointIndex);
                var (x, y, align) = PointPosition(g, script, spec, layout, pointIndex);
                DrawWrapped(g, text, spec.Point with { X = x, Y = y, Align = align }, Color.White, Color.FromArgb(185, 0, 0, 0));
            }
            else if (pointIndex == script.Points.Count && script.Cta.Length > 0)
            {
                var lines = WrapText(g, NormalizeRenderText(script.Cta), spec.Cta.FontSize, spec.Cta.Width);
                var y = CtaPositionY(g, script, spec, layout, lines);
                DrawLines(g, lines, spec.Cta with { Y = y }, Color.FromArgb(240, 255, 255, 255), Color.FromArgb(180, 0, 0, 0));
            }
        }

        var path = Path.Combine(outputFolder, $"{ScriptParser.Slugify(script.Title)}-{stamp}-{layerIndex + 1}.png");
        image.Save(path, ImageFormat.Png);
        return path;
    }

    private static void DrawPopInStayOverlay(Graphics g, ReelScript script, string layout, int layerIndex)
    {
        var readable = GetReadableLayout(script, layout);
        var bodyX = (Width - readable.BodyWidth) / 2f;
        var titleText = layout == "one_word_hook" ? FirstHookWord(script.Title) : NormalizeRenderText(script.Title);
        if (layout == "quote_style")
        {
            titleText = $"\"{titleText.Trim('"')}\"";
        }

        var titleFontSize = FitTitleFontSize(g, titleText, readable);
        var titleLines = WrapText(g, titleText, titleFontSize, readable.BodyWidth);
        DrawLines(g, titleLines, new LayoutParam(bodyX, readable.TitleY, readable.BodyWidth, "center", titleFontSize), Color.White, Color.FromArgb(210, 0, 0, 0));

        if (layerIndex == 0)
        {
            return;
        }

        var currentPointIndex = Math.Min(layerIndex - 1, script.Points.Count - 1);
        if (currentPointIndex < 0)
        {
            return;
        }

        var visiblePoints = script.Points
            .Take(currentPointIndex + 1)
            .Select(point => StripLeadingListNumber(NormalizeRenderText(point)))
            .Where(point => point.Length > 0)
            .ToList();
        if (visiblePoints.Count == 0)
        {
            return;
        }

        var fitted = FitBody(g, visiblePoints, readable.BodyWidth, SafeBottom - readable.BodyY, Math.Min(42f, titleFontSize + 2f), readable);
        DrawReadableTreatment(g, script, readable, bodyX, fitted, currentPointIndex);
        DrawReadableBody(g, script, readable, bodyX, fitted, currentPointIndex);
    }

    private static void DrawReadableTreatment(Graphics g, ReelScript script, ReadableLayout readable, float bodyX, FittedBody fitted, int currentPointIndex)
    {
        var totalHeight = BodyHeight(fitted);
        using var subtleBrush = new SolidBrush(Color.FromArgb(54, 0, 0, 0));
        using var linePen = new Pen(Color.FromArgb(120, 255, 255, 255), 2f);
        switch (readable.Variant)
        {
            case ReadableVariant.Card:
                g.FillRectangle(subtleBrush, bodyX - 52f, readable.TitleY - 28f, readable.BodyWidth + 104f, Math.Min(SafeBottom - readable.TitleY, readable.BodyY - readable.TitleY + totalHeight + 72f));
                break;
            case ReadableVariant.Divider:
                g.DrawLine(linePen, bodyX, readable.BodyY - 34f, bodyX + readable.BodyWidth, readable.BodyY - 34f);
                break;
            case ReadableVariant.Spotlight:
                var newestY = ParagraphY(fitted, fitted.Paragraphs.Count - 1, readable.BodyY);
                var newestHeight = ParagraphHeight(fitted.Paragraphs.Last(), fitted.FontSize, fitted.LineGap);
                using (var accentBrush = new SolidBrush(Color.FromArgb(44, CurrentLineColor(script))))
                {
                    g.FillRectangle(accentBrush, bodyX - 24f, newestY - 12f, readable.BodyWidth + 48f, newestHeight + 24f);
                }
                break;
        }
    }

    private static void DrawReadableBody(Graphics g, ReelScript script, ReadableLayout readable, float bodyX, FittedBody fitted, int currentPointIndex)
    {
        var accent = CurrentLineColor(script);
        var isFinalTakeaway = currentPointIndex >= script.Points.Count - 1;
        var markerX = bodyX - 34f;
        using var timelinePen = new Pen(Color.FromArgb(120, 255, 255, 255), 3f);
        if (readable.Variant is ReadableVariant.Timeline or ReadableVariant.VerticalIndicator && fitted.Paragraphs.Count > 1)
        {
            var firstY = ParagraphY(fitted, 0, readable.BodyY) + fitted.FontSize * 0.48f;
            var lastY = ParagraphY(fitted, fitted.Paragraphs.Count - 1, readable.BodyY) + fitted.FontSize * 0.48f;
            g.DrawLine(timelinePen, markerX, firstY, markerX, lastY);
        }

        for (var i = 0; i < fitted.Paragraphs.Count; i++)
        {
            var y = ParagraphY(fitted, i, readable.BodyY);
            var isNewest = i == fitted.Paragraphs.Count - 1;
            var opacity = OpacityForParagraph(readable.Variant, i, fitted.Paragraphs.Count);
            var color = isNewest && isFinalTakeaway
                ? accent
                : Color.FromArgb(opacity, 255, 255, 255);

            DrawReadableMarker(g, readable, markerX, y, i, fitted.Paragraphs.Count, isNewest, accent, opacity);

            var paragraphX = readable.Variant == ReadableVariant.LeftStory ? bodyX + 26f : bodyX;
            var paragraphWidth = readable.Variant == ReadableVariant.LeftStory ? readable.BodyWidth - 26f : readable.BodyWidth;
            DrawLines(g, fitted.Paragraphs[i], new LayoutParam(paragraphX, y, paragraphWidth, readable.BodyAlign, fitted.FontSize), color, Color.FromArgb(Math.Min(225, opacity), 0, 0, 0), fitted.LineGap);

            if (readable.Variant == ReadableVariant.Divider && i < fitted.Paragraphs.Count - 1)
            {
                using var dividerPen = new Pen(Color.FromArgb(72, 255, 255, 255), 1.5f);
                var dividerY = y + ParagraphHeight(fitted.Paragraphs[i], fitted.FontSize, fitted.LineGap) + fitted.ParagraphGap * 0.48f;
                g.DrawLine(dividerPen, bodyX, dividerY, bodyX + readable.BodyWidth, dividerY);
            }
        }
    }

    private static void DrawReadableMarker(Graphics g, ReadableLayout readable, float markerX, float y, int index, int count, bool isNewest, Color accent, int opacity)
    {
        if (readable.Variant == ReadableVariant.LeftStory)
        {
            using var brush = new SolidBrush(isNewest && index == count - 1 ? accent : Color.FromArgb(opacity, 255, 255, 255));
            g.FillEllipse(brush, markerX + 19f, y + 15f, 9f, 9f);
        }
        else if (readable.Variant == ReadableVariant.Timeline)
        {
            using var fill = new SolidBrush(Color.FromArgb(235, 0, 0, 0));
            using var outline = new Pen(isNewest ? accent : Color.FromArgb(opacity, 255, 255, 255), 3f);
            g.FillEllipse(fill, markerX - 8f, y + 9f, 16f, 16f);
            g.DrawEllipse(outline, markerX - 8f, y + 9f, 16f, 16f);
        }
        else if (readable.Variant == ReadableVariant.VerticalIndicator && isNewest)
        {
            using var pen = new Pen(accent, 5f);
            g.DrawLine(pen, markerX - 13f, y + 9f, markerX - 13f, y + Math.Max(34f, 9f + 0.8f * readable.LineGapScale * 100f));
        }
    }

    private static int OpacityForParagraph(ReadableVariant variant, int index, int count)
    {
        var distanceFromNewest = Math.Max(0, count - 1 - index);
        if (variant == ReadableVariant.MinimalFloating)
        {
            return distanceFromNewest == 0 ? 255 : 132;
        }

        return distanceFromNewest switch
        {
            0 => 255,
            1 => variant is ReadableVariant.Focus or ReadableVariant.Spotlight ? 198 : 218,
            2 => variant is ReadableVariant.Focus or ReadableVariant.Spotlight ? 164 : 184,
            _ => variant is ReadableVariant.Focus or ReadableVariant.Spotlight ? 138 : 160
        };
    }

    private static ReadableLayout GetReadableLayout(ReelScript script, string layout)
    {
        var variant = PickReadableVariant(script, layout);
        var align = variant == ReadableVariant.CenterStack || PrefersCenteredBody(script) ? "center" : "left";
        return variant switch
        {
            ReadableVariant.CenterStack => new(variant, SafeTop + 12f, Height * 0.250f, Width * 0.74f, "center", 0.38f, 1.06f),
            ReadableVariant.LeftStory => new(variant, SafeTop + 10f, Height * 0.255f, Width * 0.72f, "left", 0.36f, 1.02f),
            ReadableVariant.Focus => new(variant, SafeTop + 14f, Height * 0.255f, Width * 0.73f, align, 0.38f, 1.05f),
            ReadableVariant.Card => new(variant, SafeTop + 18f, Height * 0.265f, Width * 0.70f, align, 0.35f, 0.96f),
            ReadableVariant.Timeline => new(variant, SafeTop + 12f, Height * 0.250f, Width * 0.70f, "left", 0.34f, 0.92f),
            ReadableVariant.VerticalIndicator => new(variant, SafeTop + 16f, Height * 0.260f, Width * 0.70f, "left", 0.35f, 0.98f),
            ReadableVariant.Divider => new(variant, SafeTop + 10f, Height * 0.255f, Width * 0.75f, align, 0.32f, 0.82f),
            ReadableVariant.Spotlight => new(variant, SafeTop + 16f, Height * 0.260f, Width * 0.72f, align, 0.34f, 0.94f),
            ReadableVariant.MinimalFloating => new(variant, SafeTop + 24f, Height * 0.315f, Width * 0.68f, align, 0.42f, 1.18f),
            _ => new(variant, SafeTop + 12f, Height * 0.250f, BodyWidth, align, 0.36f, 1.02f)
        };
    }

    private static ReadableVariant PickReadableVariant(ReelScript script, string layout)
    {
        var niche = NormalizeRenderText(script.Niche).ToLowerInvariant();
        ReadableVariant[] variants = niche switch
        {
            var n when n.Contains("bible") || n.Contains("faith") || n.Contains("prayer") =>
                [ReadableVariant.CenterStack, ReadableVariant.Timeline, ReadableVariant.Focus, ReadableVariant.Card, ReadableVariant.Divider, ReadableVariant.Spotlight],
            var n when n.Contains("stoic") || n.Contains("quote") =>
                [ReadableVariant.CenterStack, ReadableVariant.Focus, ReadableVariant.Card, ReadableVariant.Divider, ReadableVariant.Spotlight],
            var n when n.Contains("communication") || n.Contains("comm") =>
                [ReadableVariant.LeftStory, ReadableVariant.Focus, ReadableVariant.Card, ReadableVariant.VerticalIndicator, ReadableVariant.Spotlight],
            var n when n.Contains("product") || n.Contains("money") || n.Contains("psychology") || n.Contains("self") =>
                [ReadableVariant.LeftStory, ReadableVariant.Focus, ReadableVariant.Card, ReadableVariant.VerticalIndicator, ReadableVariant.Divider, ReadableVariant.Spotlight, ReadableVariant.MinimalFloating],
            var n when n.Contains("gym") || n.Contains("fitness") =>
                [ReadableVariant.CenterStack, ReadableVariant.LeftStory, ReadableVariant.Focus, ReadableVariant.Card, ReadableVariant.VerticalIndicator, ReadableVariant.Spotlight],
            _ =>
                [ReadableVariant.CenterStack, ReadableVariant.LeftStory, ReadableVariant.Focus, ReadableVariant.Card, ReadableVariant.Divider, ReadableVariant.Spotlight]
        };

        return variants[Math.Abs(StableHash($"{script.Code}|{script.Title}|{layout}|{script.Niche}")) % variants.Length];
    }

    private static int StableHash(string key)
    {
        var hash = 17;
        foreach (var ch in key)
        {
            hash = unchecked(hash * 31 + ch);
        }

        return hash == int.MinValue ? 0 : hash;
    }

    private static bool PrefersCenteredBody(ReelScript script)
    {
        var niche = NormalizeRenderText(script.Niche).ToLowerInvariant();
        return niche.Contains("bible") ||
            niche.Contains("faith") ||
            niche.Contains("stoic") ||
            niche.Contains("quote") ||
            niche.Contains("prayer");
    }

    private static Color CurrentLineColor(ReelScript script)
    {
        var key = $"{script.Code}|{script.Title}";
        return CurrentLineAccentColors[Math.Abs(StableHash(key)) % CurrentLineAccentColors.Length];
    }

    private sealed record FittedBody(float FontSize, float LineGap, float ParagraphGap, List<List<string>> Paragraphs);

    private static float BodyHeight(FittedBody fitted) =>
        fitted.Paragraphs.Sum(lines => ParagraphHeight(lines, fitted.FontSize, fitted.LineGap)) +
        fitted.ParagraphGap * Math.Max(0, fitted.Paragraphs.Count - 1);

    private static float ParagraphY(FittedBody fitted, int index, float startY)
    {
        var y = startY;
        for (var i = 0; i < index; i++)
        {
            y += ParagraphHeight(fitted.Paragraphs[i], fitted.FontSize, fitted.LineGap) + fitted.ParagraphGap;
        }

        return y;
    }

    private static float FitTitleFontSize(Graphics g, string title, ReadableLayout readable)
    {
        for (var fontSize = 66f; fontSize >= 52f; fontSize -= 2f)
        {
            if (TextBlockHeight(g, title, fontSize, readable.BodyWidth) <= readable.BodyY - readable.TitleY - 34f)
            {
                return fontSize;
            }
        }

        return 52f;
    }

    private static FittedBody FitBody(Graphics g, IReadOnlyList<string> points, float width, float maxHeight, float startFontSize, ReadableLayout readable)
    {
        for (var fontSize = startFontSize; fontSize >= 26f; fontSize -= 2f)
        {
            var lineGap = Math.Max(10f, fontSize * readable.LineGapScale);
            var paragraphGap = Math.Max(30f, fontSize * readable.ParagraphGapScale);
            var paragraphs = points.Select(point => WrapText(g, point, fontSize, width)).ToList();
            var height = paragraphs.Sum(lines => ParagraphHeight(lines, fontSize, lineGap)) + paragraphGap * Math.Max(0, paragraphs.Count - 1);
            if (height <= maxHeight)
            {
                return new FittedBody(fontSize, lineGap, paragraphGap, paragraphs);
            }
        }

        var minimumLineGap = 8f;
        var minimumParagraphGap = 22f;
        return new FittedBody(26f, minimumLineGap, minimumParagraphGap, points.Select(point => WrapText(g, point, 26f, width)).ToList());
    }

    public static LayoutSpec GetLayoutSpec(string layout) =>
        layout switch
        {
            "two_column_split" => Spec(80, 320, 420, "left", 72, 560, 500, 360, "left", 44, 560, 1470, 360, "left", 30),
            "grid_layout" => Spec(100, 250, 880, "center", 72, 110, 640, 380, "left", 42, 100, 1520, 880, "center", 30),
            "masonry_layout" => Spec(90, 260, 900, "left", 70, 95, 630, 430, "left", 40, 100, 1510, 880, "left", 30, "- "),
            "hero_list" => Spec(80, 300, 920, "center", 82, 120, 860, 840, "left", 46, 110, 1515, 860, "center", 30, "- "),
            "alternating_rows" => Spec(100, 260, 880, "center", 72, 110, 680, 860, "left", 44, 110, 1510, 860, "center", 30),
            "sidebar_layout" => Spec(90, 320, 640, "left", 74, 120, 760, 610, "left", 42, 120, 1490, 610, "left", 30, "- "),
            "collage_layout" => Spec(90, 280, 900, "center", 76, 120, 700, 360, "left", 38, 110, 1510, 860, "center", 30),
            "auto_fit_tiles" => Spec(90, 280, 900, "center", 74, 110, 680, 380, "left", 40, 100, 1510, 880, "center", 30),
            "tabbed_layout" => Spec(90, 330, 900, "left", 72, 120, 770, 840, "left", 44, 110, 1495, 860, "left", 30, "> "),
            "magazine_layout" => Spec(90, 250, 900, "left", 80, 120, 760, 540, "left", 42, 120, 1510, 840, "left", 30),
            "template_rotation_layout" => Spec(90, 470, 900, "center", 80, 150, 820, 780, "center", 48, 110, 1515, 860, "center", 30),
            "priority_based_layout" => Spec(80, 310, 920, "center", 88, 120, 920, 840, "center", 42, 100, 1510, 880, "center", 30),
            "adaptive_smart_layout" => Spec(90, 360, 900, "center", 76, 120, 760, 840, "center", 46, 110, 1510, 860, "center", 30),
            "fallback_universal_layout" => Spec(90, 520, 900, "center", 82, 150, 820, 780, "center", 48, 110, 1510, 860, "center", 30),
            "question_answer" => Spec(105, 500, 870, "center", 76, 145, 825, 790, "center", 52, 110, 1515, 860, "center", 33),
            "left_stack" => Spec(92, 350, 820, "left", 72, 105, 710, 850, "left", 48, 105, 1510, 850, "left", 33, "- "),
            "right_stack" => Spec(155, 680, 820, "right", 70, 155, 930, 820, "right", 46, 155, 1510, 820, "right", 33),
            "list_style" => Spec(90, 340, 900, "left", 70, 110, 690, 850, "left", 48, 105, 1510, 850, "left", 33, "- "),
            "top_bottom" => Spec(90, 250, 900, "left", 68, 90, 1180, 900, "left", 54, 90, 1510, 900, "left", 33),
            "one_word_hook" => Spec(80, 420, 920, "center", 118, 145, 760, 790, "left", 48, 110, 1510, 860, "center", 33, "- "),
            "quote_style" => Spec(90, 660, 900, "center", 78, 125, 1080, 830, "center", 42, 110, 1450, 860, "center", 33),
            "story_block" => Spec(90, 270, 900, "left", 68, 100, 560, 880, "left", 43, 100, 1510, 880, "left", 33),
            "progress_reveal" => Spec(90, 420, 900, "left", 54, 90, 770, 900, "center", 92, 110, 1510, 860, "center", 33),
            "center_card" => Spec(100, 480, 880, "center", 70, 155, 830, 770, "center", 50, 110, 1510, 860, "center", 33),
            _ => Spec(90, 520, 900, "center", 84, 150, 790, 780, "center", 50, 110, 1510, 860, "center", 33)
        };

    private static LayoutSpec Spec(float tx, float ty, float tw, string ta, float ts, float px, float py, float pw, string pa, float ps, float cx, float cy, float cw, string ca, float cs, string marker = "") =>
        new(new(tx, ty, tw, ta, ts), new(px, py, pw, pa, ps), new(cx, cy, cw, ca, cs), marker);

    private static void DrawWrapped(Graphics g, string text, LayoutParam param, Color color, Color shadow)
    {
        var lines = WrapText(g, NormalizeRenderText(text), param.FontSize, param.Width);
        DrawLines(g, lines, param, color, shadow);
    }

    private static void DrawLines(Graphics g, IReadOnlyList<string> lines, LayoutParam param, Color color, Color shadow, float? lineGap = null)
    {
        using var font = new Font("Arial", param.FontSize, FontStyle.Bold, GraphicsUnit.Pixel);
        using var shadowBrush = new SolidBrush(shadow);
        using var brush = new SolidBrush(color);
        var y = param.Y;
        var gap = lineGap ?? 12f;
        foreach (var line in lines)
        {
            var size = g.MeasureString(line, font, new PointF(0, 0), StringFormat.GenericTypographic);
            var x = param.Align switch
            {
                "left" => param.X,
                "right" => param.X + param.Width - size.Width,
                _ => param.X + (param.Width - size.Width) / 2f
            };

            foreach (var (dx, dy) in StrokeOffsets())
            {
                g.DrawString(line, font, shadowBrush, x + dx, y + dy, StringFormat.GenericTypographic);
            }

            g.DrawString(line, font, brush, x, y, StringFormat.GenericTypographic);
            y += size.Height + gap;
        }
    }

    private static IEnumerable<(float X, float Y)> StrokeOffsets()
    {
        yield return (-3f, 0f);
        yield return (3f, 0f);
        yield return (0f, -3f);
        yield return (0f, 3f);
        yield return (-2f, -2f);
        yield return (2f, -2f);
        yield return (-2f, 2f);
        yield return (2f, 2f);
        yield return (3f, 4f);
    }

    private static List<string> WrapText(Graphics g, string text, float fontSize, float maxWidth)
    {
        using var font = new Font("Arial", fontSize, FontStyle.Bold, GraphicsUnit.Pixel);
        var lines = new List<string>();
        var current = "";
        foreach (var word in text.Split(' ', StringSplitOptions.RemoveEmptyEntries))
        {
            var candidate = current.Length == 0 ? word : $"{current} {word}";
            var width = g.MeasureString(candidate, font, new PointF(0, 0), StringFormat.GenericTypographic).Width;
            if (width > maxWidth && current.Length > 0)
            {
                lines.Add(current);
                current = word;
            }
            else
            {
                current = candidate;
            }
        }

        if (current.Length > 0)
        {
            lines.Add(current);
        }

        return lines;
    }

    private static string PointText(ReelScript script, LayoutSpec spec, string layout, int pointIndex)
    {
        var point = NormalizeRenderText(script.Points[pointIndex]);
        if (layout == "progress_reveal")
        {
            return StripLeadingListNumber(point);
        }

        if (spec.Marker == "- " && ScriptPrefersNumberedList(script))
        {
            var parsed = ParseLeadingListNumber(point);
            return parsed is null ? $"{pointIndex + 1}. {point}" : $"{parsed.Number}. {parsed.Rest}";
        }

        return NormalizeRenderText($"{spec.Marker}{point}");
    }

    private static (float X, float Y, string Align) PointPosition(Graphics g, ReelScript script, LayoutSpec spec, string layout, int pointIndex)
    {
        var blockGap = Math.Max(spec.Point.FontSize * 0.45f, 26f);

        if (layout is "grid_layout" or "masonry_layout" or "collage_layout" or "auto_fit_tiles")
        {
            var columnGap = layout is "masonry_layout" or "collage_layout" ? 465f : 470f;
            var rowOffset = layout == "masonry_layout" ? 58f : layout == "collage_layout" ? 46f : 0f;
            var leftY = spec.Point.Y;
            var rightY = spec.Point.Y;
            for (var i = 0; i < pointIndex; i++)
            {
                var h = TextBlockHeight(g, PointText(script, spec, layout, i), spec.Point.FontSize, spec.Point.Width) + blockGap;
                if (i % 2 == 0) leftY += h; else rightY += h;
            }

            return pointIndex % 2 == 0
                ? (spec.Point.X, leftY, spec.Point.Align)
                : (spec.Point.X + columnGap, rightY + rowOffset, spec.Point.Align);
        }

        var y = spec.Point.Y;
        for (var i = 0; i < pointIndex; i++)
        {
            y += TextBlockHeight(g, PointText(script, spec, layout, i), spec.Point.FontSize, spec.Point.Width) + blockGap;
        }

        return layout == "alternating_rows" && pointIndex % 2 == 1
            ? (spec.Point.X + 120, y, "right")
            : (spec.Point.X, y, spec.Point.Align);
    }

    private static float CtaPositionY(Graphics g, ReelScript script, LayoutSpec spec, string layout, IReadOnlyList<string> ctaLines)
    {
        var pointBottom = 0f;
        for (var i = 0; i < script.Points.Count; i++)
        {
            var (_, y, _) = PointPosition(g, script, spec, layout, i);
            pointBottom = Math.Max(pointBottom, y + TextBlockHeight(g, PointText(script, spec, layout, i), spec.Point.FontSize, spec.Point.Width));
        }

        var gap = Math.Max(spec.Point.FontSize * 0.55f, 34f);
        var desiredY = pointBottom + gap;
        var ctaHeight = ctaLines.Count * (spec.Cta.FontSize + 12f);
        var maxY = Height - ctaHeight - 105;
        return Math.Min(Math.Max(desiredY, spec.Cta.Y), maxY);
    }

    private static float TextBlockHeight(Graphics g, string text, float fontSize, float width) =>
        WrapText(g, text, fontSize, width).Count * (fontSize + 12f);

    private static float ParagraphHeight(IReadOnlyList<string> lines, float fontSize, float lineGap) =>
        lines.Count == 0 ? 0 : lines.Count * fontSize + Math.Max(0, lines.Count - 1) * lineGap;

    private static bool ScriptPrefersNumberedList(ReelScript script)
    {
        var first = NormalizeRenderText(script.Title).FirstOrDefault();
        return char.IsAsciiDigit(first) ||
            script.Points.Select(NormalizeRenderText).Any(point => ParseLeadingListNumber(point) is not null);
    }

    private sealed record LeadingListNumber(int Number, string Rest);

    private static LeadingListNumber? ParseLeadingListNumber(string value)
    {
        var trimmed = value.TrimStart();
        if (trimmed.StartsWith('-'))
        {
            trimmed = trimmed[1..].TrimStart();
        }

        var match = Regex.Match(trimmed, @"^(\d+)(?:[\s.)]+)(.+)$");
        return match.Success ? new LeadingListNumber(int.Parse(match.Groups[1].Value), match.Groups[2].Value.TrimStart()) : null;
    }

    private static string StripLeadingListNumber(string value) =>
        ParseLeadingListNumber(value)?.Rest ?? value;

    private static string FirstHookWord(string value)
    {
        var normalized = NormalizeRenderText(value);
        var match = Regex.Match(normalized, @"[A-Za-z][A-Za-z'-]{2,}");
        return match.Success ? $"{match.Value.ToUpperInvariant()}?" : normalized.ToUpperInvariant();
    }

    private static string NormalizeRenderText(string value)
    {
        var repaired = RepairMojibake(value);
        var chars = repaired.Select(ch => ch switch
        {
            '\u2010' or '\u2011' or '\u2012' or '\u2013' or '\u2014' or '\u2212' => '-',
            '\u2018' or '\u2019' or '\u201B' => '\'',
            '\u201C' or '\u201D' or '\u201F' => '"',
            '\u00A0' or '\uFE0F' or '\u20E3' or '\u200D' => ' ',
            _ => ch
        });
        return string.Join(" ", new string(chars.ToArray()).Split(' ', StringSplitOptions.RemoveEmptyEntries));
    }

    private static string RepairMojibake(string value)
    {
        if (!value.Contains('â') && !value.Contains('ð') && !value.Contains('Ã'))
        {
            return value;
        }

        try
        {
            var bytes = Encoding.GetEncoding(1252).GetBytes(value);
            var fixedText = Encoding.UTF8.GetString(bytes);
            return fixedText.Contains('â') || fixedText.Contains('Ã') ? value : fixedText;
        }
        catch
        {
            return value;
        }
    }
}

internal sealed record RenderOptions
{
    public required string ScriptPath { get; init; }
    public required string VideosFolder { get; init; }
    public required string MusicFolder { get; init; }
    public required string OutputFolder { get; init; }
    public required string OverlayFolder { get; init; }
    public float Duration { get; init; } = Renderer.DefaultReelDuration;
    public string Workers { get; init; } = "auto";
    public BlurStrength Blur { get; init; } = BlurStrength.None;
    public string? ErrorLogPath { get; init; }
    public string? TimingLogPath { get; init; }
}

internal sealed record RenderSummary(int SuccessCount, int TotalCount)
{
    public bool FullySuccessful => TotalCount > 0 && SuccessCount == TotalCount;
}

internal static class Renderer
{
    public const float MinReelDuration = 10.0f;
    public const float DefaultReelDuration = 12.0f;
    public const float MaxReelDuration = 24.9f;
    private static readonly Random Random = new();
    private static readonly object TimingFileLock = new();

    public static RenderSummary RenderSource(RenderOptions options, CancellationToken cancellationToken, Action<string> log)
    {
        var scriptSources = CollectScriptSources(options.ScriptPath);
        var batchMode = Directory.Exists(options.ScriptPath);
        var grandSuccess = 0;
        var grandTotal = 0;
        var batchStartedAt = Stopwatch.StartNew();

        AppendTimingLine(options.TimingLogPath, $"START BATCH | {DateTime.Now:yyyy-MM-dd HH:mm:ss} | files={scriptSources.Count} | source={options.ScriptPath}");

        for (var i = 0; i < scriptSources.Count; i++)
        {
            cancellationToken.ThrowIfCancellationRequested();
            var scriptFile = scriptSources[i];
            var scriptFileStartedAt = Stopwatch.StartNew();
            var outputRoot = batchMode ? Path.Combine(options.OutputFolder, ScriptParser.Slugify(Path.GetFileNameWithoutExtension(scriptFile))) : options.OutputFolder;
            var overlayRoot = batchMode ? Path.Combine(options.OverlayFolder, ScriptParser.Slugify(Path.GetFileNameWithoutExtension(scriptFile))) : options.OverlayFolder;
            var summary = new RenderSummary(0, 0);
            var status = "DONE";
            var errorMessage = "";
            var shouldRethrow = false;

            if (batchMode)
            {
                log($"Batch file {i + 1}/{scriptSources.Count}");
            }
            log($"Script source: {scriptFile}");
            log($"Video output: {outputRoot}");
            AppendTimingLine(options.TimingLogPath, $"START SCRIPT FILE | {DateTime.Now:yyyy-MM-dd HH:mm:ss} | {i + 1}/{scriptSources.Count} | {Path.GetFileName(scriptFile)}");

            try
            {
                summary = RenderScriptFile(options with { ScriptPath = scriptFile, OutputFolder = outputRoot, OverlayFolder = overlayRoot }, cancellationToken, log);
                grandSuccess += summary.SuccessCount;
                grandTotal += summary.TotalCount;
                if (!summary.FullySuccessful)
                {
                    status = "PARTIAL";
                    AppendError(options.ErrorLogPath, scriptFile, $"Partial failure: {summary.SuccessCount}/{summary.TotalCount} reels succeeded");
                }
                else
                {
                    AppendCompletionLedger(scriptFile, log);
                }

                if (batchMode)
                {
                    log($"Completed batch file {i + 1}/{scriptSources.Count} -> {scriptFile} ({summary.SuccessCount} / {summary.TotalCount} reels succeeded in this file)");
                }
            }
            catch (Exception ex)
            {
                status = "ERROR";
                errorMessage = ex.Message;
                AppendError(options.ErrorLogPath, scriptFile, ex.Message);
                log($"Error: {ex.Message}");
                if (!batchMode)
                {
                    shouldRethrow = true;
                }
            }

            var completedFiles = i + 1;
            var fileElapsed = scriptFileStartedAt.Elapsed;
            var averageMinutesPerFile = batchStartedAt.Elapsed.TotalMinutes / Math.Max(completedFiles, 1);
            var remaining = TimeSpan.FromMinutes(Math.Max(scriptSources.Count - completedFiles, 0) * averageMinutesPerFile);
            var timingLine = $"SCRIPT FILE {completedFiles}/{scriptSources.Count} | {status} | {Path.GetFileName(scriptFile)} | file_elapsed={FormatDuration(fileElapsed)} | avg_min_per_file={averageMinutesPerFile:0.00} | eta={FormatDuration(remaining)} | reels={summary.SuccessCount}/{summary.TotalCount}";
            if (!string.IsNullOrWhiteSpace(errorMessage))
            {
                timingLine += $" | error={errorMessage}";
            }
            log($"[Timing] {timingLine}");
            AppendTimingLine(options.TimingLogPath, timingLine);
            if (shouldRethrow)
            {
                throw new InvalidOperationException(errorMessage);
            }
        }

        if (batchMode)
        {
            log("==================================================");
            log($"Batch finished: rendered {grandSuccess}/{grandTotal} reels across {scriptSources.Count} script file(s).");
        }

        AppendTimingLine(options.TimingLogPath, $"END BATCH | {DateTime.Now:yyyy-MM-dd HH:mm:ss} | rendered={grandSuccess}/{grandTotal} | total_elapsed={FormatDuration(batchStartedAt.Elapsed)}");
        return new RenderSummary(grandSuccess, grandTotal);
    }

    private static RenderSummary RenderScriptFile(RenderOptions options, CancellationToken cancellationToken, Action<string> log)
    {
        var scripts = ScriptParser.ParseFile(options.ScriptPath);
        log($"Loaded {scripts.Count} script(s) from {options.ScriptPath}");
        if (scripts.Count == 0)
        {
            return new RenderSummary(0, 0);
        }

        var videos = ListFiles(options.VideosFolder, [".mp4", ".mov", ".mkv", ".webm"]).OrderBy(_ => Random.Next()).ToList();
        var music = ListFiles(options.MusicFolder, [".mp3", ".wav", ".m4a", ".aac", ".mp4", ".mov", ".mkv", ".webm"]).OrderBy(_ => Random.Next()).ToList();
        log(videos.Count == 0 ? "No background videos found; rendering on solid black background." : $"Found {videos.Count} background video(s)");
        log(music.Count == 0 ? "No music tracks found; rendering silent videos." : $"Found {music.Count} music track(s)");
        log($"Blur mode: {options.Blur.ToArg()}");

        var workers = ParseWorkers(options.Workers, scripts.Count);
        var ffmpegThreads = FfmpegThreadsForWorkers(workers);
        log($"Spinning up worker pool ({workers} parallel workers)...");
        log($"FFmpeg thread budget per worker: {ffmpegThreads}");

        var success = 0;
        var startedAt = Stopwatch.StartNew();
        var results = new ConcurrentBag<(int Index, string? Path, string? Error)>();
        var parallelOptions = new ParallelOptions { MaxDegreeOfParallelism = workers, CancellationToken = cancellationToken };

        Parallel.ForEach(scripts.Select((script, index) => (script, index)), parallelOptions, item =>
        {
            try
            {
                var script = ScriptParser.CollapseDuplicateTitlePoint(item.script);
                var stamp = MillisecondStamp();
                var duration = ResolveReelDuration(script, options.Duration);
                log($"[Worker] Rendering script {item.index + 1}/{scripts.Count} from {Path.GetFileName(options.ScriptPath)}: \"{script.Title}\"");

                var overlays = new List<string> { OverlayRenderer.MakeOverlay(script, 0, options.OverlayFolder, stamp) };
                for (var pointIndex = 0; pointIndex < script.Points.Count; pointIndex++)
                {
                    cancellationToken.ThrowIfCancellationRequested();
                    overlays.Add(OverlayRenderer.MakeOverlay(script, pointIndex + 1, options.OverlayFolder, stamp));
                }

                if (script.Cta.Length > 0)
                {
                    overlays.Add(OverlayRenderer.MakeOverlay(script, script.Points.Count + 1, options.OverlayFolder, stamp));
                }

                var output = FfmpegRenderer.RenderVideo(script, item.index, videos, music, options.OutputFolder, overlays, duration, ffmpegThreads, options.Blur, cancellationToken);
                Interlocked.Increment(ref success);
                results.Add((item.index, output, null));
            }
            catch (Exception ex)
            {
                results.Add((item.index, null, ex.Message));
            }
        });

        log("--------------------------------------------------");
        log("RENDERING DONE");
        log("--------------------------------------------------");
        log($"Total elapsed time: {startedAt.Elapsed}");
        foreach (var result in results.OrderBy(r => r.Index))
        {
            log(result.Path is not null
                ? $"  Reel {result.Index + 1}: {Path.GetFileName(result.Path)}"
                : $"  Reel {result.Index + 1}: Error -> {result.Error}");
        }
        log($"Rendered {success}/{scripts.Count} videos successfully.");
        return new RenderSummary(success, scripts.Count);
    }

    public static List<string> CollectScriptSources(string scriptPath)
    {
        if (File.Exists(scriptPath))
        {
            return [Path.GetFullPath(scriptPath)];
        }

        if (!Directory.Exists(scriptPath))
        {
            throw new InvalidOperationException($"Script path does not exist: {scriptPath}");
        }

        var files = Directory.EnumerateFiles(scriptPath, "*.txt", SearchOption.AllDirectories)
            .OrderBy(path => path, new NaturalStringComparer())
            .ToList();
        if (files.Count == 0)
        {
            throw new InvalidOperationException($"No .txt script files found in {scriptPath}");
        }

        return files;
    }

    public static List<string> ListFiles(string folder, string[] extensions)
    {
        if (!Directory.Exists(folder))
        {
            return [];
        }

        return Directory.EnumerateFiles(folder, "*.*", SearchOption.AllDirectories)
            .Where(path => extensions.Contains(Path.GetExtension(path).ToLowerInvariant()))
            .OrderBy(path => path, new NaturalStringComparer())
            .ToList();
    }

    private static int ParseWorkers(string value, int scriptCount)
    {
        var available = Math.Max(Environment.ProcessorCount, 2);
        if (value.Trim().Equals("auto", StringComparison.OrdinalIgnoreCase))
        {
            return Math.Max(1, Math.Min(available, Math.Max(1, scriptCount)));
        }

        return int.TryParse(value, out var fixedWorkers) ? Math.Max(1, Math.Min(fixedWorkers, Math.Max(1, scriptCount))) : Math.Min(4, Math.Max(1, scriptCount));
    }

    private static int FfmpegThreadsForWorkers(int workers) => Math.Clamp(Math.Max(1, Environment.ProcessorCount) / Math.Max(1, workers), 1, 4);

    private sealed record DurationRange(float Min, float Max);

    private static float ResolveReelDuration(ReelScript script, float defaultDuration)
    {
        var range = DurationRangeFor(script);
        var requested = script.Duration ?? defaultDuration;
        if (requested >= range.Min && requested <= range.Max)
        {
            return Math.Clamp(requested, MinReelDuration, MaxReelDuration);
        }

        var natural = PickDurationInRange(script, range);
        if (requested > range.Max)
        {
            return Math.Clamp(requested, range.Min, MaxReelDuration);
        }

        return natural;
    }

    private static DurationRange DurationRangeFor(ReelScript script)
    {
        var niche = NormalizeLoose(script.Niche);
        var storyLike = script.Points.Count >= 6 || script.Points.Sum(point => point.Length) >= 430;
        return niche switch
        {
            var n when n.Contains("bible") || n.Contains("faith") || n.Contains("prayer") => new(18.6f, 24.9f),
            var n when n.Contains("stoic") => new(18.2f, 22.4f),
            var n when n.Contains("product") => new(18.1f, 19.6f),
            var n when n.Contains("psychology") || n.Contains("money") => new(18.1f, 20.8f),
            var n when n.Contains("self") || n.Contains("relationship") || n.Contains("parent") => new(18.4f, 22.5f),
            var n when n.Contains("communication") || n.Contains("comm") => new(18.1f, 20.8f),
            var n when n.Contains("gym") || n.Contains("fitness") => storyLike ? new(18.2f, 20.6f) : new(18.1f, 19.4f),
            _ => new(18.1f, 22.4f)
        };
    }

    private static float PickDurationInRange(ReelScript script, DurationRange range)
    {
        var hash = StableHash($"{script.Code}|{script.Title}|{script.Niche}|duration");
        var unit = (Math.Abs(hash) % 10000) / 9999f;
        var seconds = range.Min + (range.Max - range.Min) * unit;
        return MathF.Round(Math.Clamp(seconds, MinReelDuration, MaxReelDuration) * 10f) / 10f;
    }

    private static int StableHash(string key)
    {
        var hash = 17;
        foreach (var ch in key)
        {
            hash = unchecked(hash * 31 + ch);
        }

        return hash == int.MinValue ? 0 : hash;
    }

    private static string NormalizeLoose(string value) =>
        string.Join(" ", value.Select(ch => char.IsLetterOrDigit(ch) ? char.ToLowerInvariant(ch) : ' ')
            .Aggregate(new StringBuilder(), (sb, ch) => sb.Append(ch), sb => sb.ToString())
            .Split(' ', StringSplitOptions.RemoveEmptyEntries));

    private static string MillisecondStamp()
    {
        var ms = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds().ToString();
        return ms.Length > 8 ? ms[^8..] : ms;
    }

    private static void AppendError(string? errorLogPath, string scriptFile, string message)
    {
        if (string.IsNullOrWhiteSpace(errorLogPath))
        {
            return;
        }

        var parent = Path.GetDirectoryName(errorLogPath);
        if (!string.IsNullOrWhiteSpace(parent))
        {
            Directory.CreateDirectory(parent);
        }
        File.AppendAllText(errorLogPath, $"{scriptFile} | {message}{Environment.NewLine}", Encoding.UTF8);
    }

    private static void AppendTimingLine(string? timingLogPath, string message)
    {
        if (string.IsNullOrWhiteSpace(timingLogPath))
        {
            return;
        }

        var parent = Path.GetDirectoryName(timingLogPath);
        if (!string.IsNullOrWhiteSpace(parent))
        {
            Directory.CreateDirectory(parent);
        }

        lock (TimingFileLock)
        {
            File.AppendAllText(timingLogPath, message + Environment.NewLine, Encoding.UTF8);
        }
    }

    private static string FormatDuration(TimeSpan value) =>
        value.TotalHours >= 1
            ? $"{(int)value.TotalHours}:{value.Minutes:00}:{value.Seconds:00}"
            : $"{value.Minutes:00}:{value.Seconds:00}";

    private static void AppendCompletionLedger(string scriptFile, Action<string> log)
    {
        try
        {
            var parent = Path.GetDirectoryName(AppPaths.CompletionLedgerPath);
            if (parent is not null)
            {
                Directory.CreateDirectory(parent);
            }

            var week = Path.GetFileName(Path.GetDirectoryName(scriptFile)) ?? "root";
            var label = $"{Path.GetFileNameWithoutExtension(scriptFile)}:{week}";
            File.AppendAllText(AppPaths.CompletionLedgerPath, $"{label}{Environment.NewLine}", Encoding.UTF8);
            log($"Completion ledger updated: {label}");
        }
        catch (Exception ex)
        {
            log($"Completion ledger warning: {ex.Message}");
        }
    }
}

internal static class FfmpegRenderer
{
    private enum BackgroundVariant { Normal, TintOnly, GradientTint, CardOverlay }
    private sealed record Treatment(BackgroundVariant Variant, int? BlurSigma, float TintOpacity, float SecondaryTintOpacity, float CardOpacity);
    private sealed record BlurBand(int SigmaMin, int SigmaMax, int TintMin, int TintMax);
    private static readonly Random Random = new();

    public static string RenderVideo(ReelScript script, int index, IReadOnlyList<string> videos, IReadOnlyList<string> musicFiles, string outputFolder, IReadOnlyList<string> overlayPaths, float duration, int ffmpegThreads, BlurStrength blurStrength, CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        var videoPath = script.Video ?? (videos.Count > 0 ? videos[index % videos.Count] : null);
        var musicPath = script.Audio ?? (musicFiles.Count > 0 ? musicFiles[index % musicFiles.Count] : null);
        var revealStarts = BuildRevealStarts(script, overlayPaths.Count, duration);
        var effectiveDuration = Math.Max(duration, (revealStarts.LastOrDefault() + 2f));
        var treatment = ChooseTreatment(blurStrength);

        var args = new List<string> { "-y" };
        if (videoPath is not null)
        {
            args.AddRange(["-stream_loop", "-1", "-i", videoPath]);
        }
        else
        {
            args.AddRange(["-f", "lavfi", "-i", $"color=c=black:s={OverlayRenderer.Width}x{OverlayRenderer.Height}:d={effectiveDuration:0.###}"]);
        }

        foreach (var overlay in overlayPaths)
        {
            args.AddRange(["-loop", "1", "-t", effectiveDuration.ToString("0.###"), "-i", overlay]);
        }

        if (musicPath is not null)
        {
            var trackDuration = ProbeDuration(musicPath);
            if (trackDuration is > 0)
            {
                var maxStart = Math.Max(0, trackDuration.Value - Math.Max(effectiveDuration, 15f));
                if (maxStart > 0)
                {
                    args.AddRange(["-ss", (Random.NextDouble() * maxStart).ToString("0.000")]);
                }
            }
            args.AddRange(["-stream_loop", "-1", "-i", musicPath]);
        }

        var filters = new List<string>();
        var bgChain = videoPath is not null
            ? $"[0:v]scale=w={OverlayRenderer.Width}:h={OverlayRenderer.Height}:force_original_aspect_ratio=increase,crop={OverlayRenderer.Width}:{OverlayRenderer.Height}"
            : "[0:v]null";
        bgChain = ApplyTreatment(bgChain, treatment) + "[bg]";
        filters.Add(bgChain);

        var current = "[bg]";
        for (var i = 0; i < overlayPaths.Count; i++)
        {
            var inputIndex = i + 1;
            var start = Math.Round(revealStarts.ElementAtOrDefault(i) * 30d) / 30d;
            var faded = $"ovr_faded_{i}";
            var next = $"bg_next_{i}";
            filters.Add(i == 0 || script.AllAtOnce
                ? $"[{inputIndex}:v]null[{faded}]"
                : $"[{inputIndex}:v]fade=t=in:st={start:0.000}:d=0.1:alpha=1[{faded}]");
            var enable = i > 0 && !script.AllAtOnce ? $":enable='gte(t,{start:0.000})'" : "";
            filters.Add($"{current}[{faded}]overlay=0:0{enable}[{next}]");
            current = $"[{next}]";
        }

        var lastVideo = overlayPaths.Count == 0 ? "[bg]" : current;
        args.AddRange(["-filter_complex", string.Join(";", filters), "-map", lastVideo]);
        if (musicPath is not null)
        {
            args.AddRange(["-map", $"{overlayPaths.Count + 1}:a", "-c:a", "aac", "-shortest"]);
        }

        Directory.CreateDirectory(outputFolder);
        var safeTitle = ScriptParser.Slugify(script.Code.Length > 0 ? script.Code : script.Title);
        var outputPath = Path.Combine(outputFolder, $"{safeTitle}.mp4");
        args.AddRange(["-threads", Math.Max(1, ffmpegThreads).ToString(), "-c:v", "libx264", "-preset", "veryfast", "-pix_fmt", "yuv420p", "-movflags", "+faststart", "-t", effectiveDuration.ToString("0.###"), outputPath]);

        var result = RunProcess("ffmpeg", args, cancellationToken);
        if (result.ExitCode != 0)
        {
            throw new InvalidOperationException($"FFmpeg failed: {result.Output}");
        }

        return outputPath;
    }

    private static List<float> BuildRevealStarts(ReelScript script, int overlayCount, float duration)
    {
        var starts = new List<float>(overlayCount);
        if (script.AllAtOnce)
        {
            starts.AddRange(Enumerable.Repeat(0f, overlayCount));
            return starts;
        }

        if (overlayCount <= 0)
        {
            return starts;
        }

        starts.Add(0f);
        var revealCount = overlayCount - 1;
        if (revealCount == 0)
        {
            return starts;
        }

        var seed = StableHash($"{script.Code}|{script.Title}|timing");
        var firstLineAt = 0.72f + (Math.Abs(seed) % 22) / 100f;
        starts.Add(firstLineAt);
        if (revealCount == 1)
        {
            return starts;
        }

        var finalHold = Math.Clamp(duration * 0.23f, 2.7f, 3.8f);
        var finalRevealAt = Math.Clamp(duration - finalHold, Math.Min(7.8f, duration * 0.72f), Math.Max(1.2f, duration - 2.2f));
        finalRevealAt = Math.Max(finalRevealAt, firstLineAt + 0.9f);
        var remainingRevealCount = revealCount - 1;
        var weights = new List<float>(remainingRevealCount);
        for (var i = 0; i < remainingRevealCount; i++)
        {
            var jitter = (((seed >> ((i % 4) * 7)) & 0x7F) / 127f) * 0.34f - 0.17f;
            var lateWeight = 1f + i * 0.045f;
            weights.Add(Math.Max(0.72f, lateWeight + jitter));
        }

        var totalWeight = weights.Sum();
        var current = firstLineAt;
        var remainingWindow = Math.Max(0.9f, finalRevealAt - firstLineAt);
        for (var i = 0; i < remainingRevealCount; i++)
        {
            current += remainingWindow * weights[i] / totalWeight;
            starts.Add(current);
        }

        for (var i = 1; i < starts.Count; i++)
        {
            var pointIndex = i - 1;
            var pauseCount = pointIndex < script.Points.Count
                ? script.PointPauseCountsBefore.ElementAtOrDefault(pointIndex)
                : script.CtaPauseCountBefore;
            if (pauseCount <= 0)
            {
                continue;
            }

            for (var j = i; j < starts.Count; j++)
            {
                starts[j] += pauseCount * 0.45f;
            }
        }

        return starts;
    }

    private static int StableHash(string key)
    {
        var hash = 17;
        foreach (var ch in key)
        {
            hash = unchecked(hash * 31 + ch);
        }

        return hash == int.MinValue ? 0 : hash;
    }

    private static float? ProbeDuration(string path)
    {
        try
        {
            var result = RunProcess("ffprobe", ["-v", "error", "-show_entries", "format=duration", "-of", "default=noprint_wrappers=1:nokey=1", path], CancellationToken.None);
            return result.ExitCode == 0 && float.TryParse(result.Output.Trim(), out var duration) && duration > 0 ? duration : null;
        }
        catch
        {
            return null;
        }
    }

    private static Treatment ChooseTreatment(BlurStrength blurStrength)
    {
        BlurBand Light() => new(3, 5, 18, 25);
        BlurBand Middle() => new(6, 8, 26, 34);
        BlurBand Heavy() => new(9, 12, 35, 42);
        BlurBand RandomBand() => Random.Next(100) switch { < 50 => Light(), < 80 => Middle(), _ => Heavy() };
        float Opacity(int min, int max) => Random.Next(min, max + 1) / 100f;
        Treatment FromBand(BlurBand band)
        {
            var roll = Random.Next(100);
            var sigma = Random.Next(band.SigmaMin, band.SigmaMax + 1);
            var tint = Opacity(band.TintMin, band.TintMax);
            return roll < 12
                ? new(BackgroundVariant.GradientTint, sigma, Math.Max(tint - 0.05f, 0.12f), Math.Min(tint + 0.07f, 0.48f), 0)
                : roll < 24
                    ? new(BackgroundVariant.CardOverlay, sigma, Math.Max(tint - 0.08f, 0.10f), 0, Opacity(12, 22))
                    : new(BackgroundVariant.Normal, sigma, tint, 0, 0);
        }

        if (blurStrength == BlurStrength.None)
        {
            var roll = Random.Next(100);
            if (roll < 10)
            {
                return new(BackgroundVariant.TintOnly, null, Opacity(18, 42), 0, 0);
            }

            var band = RandomBand();
            var tint = Opacity(band.TintMin, band.TintMax);
            var sigma = Random.Next(band.SigmaMin, band.SigmaMax + 1);
            return roll < 20
                ? new(BackgroundVariant.GradientTint, sigma, Math.Max(tint - 0.06f, 0.12f), Math.Min(tint + 0.09f, 0.48f), 0)
                : roll < 30
                    ? new(BackgroundVariant.CardOverlay, sigma, Math.Max(tint - 0.10f, 0.10f), 0, Opacity(12, 22))
                    : new(BackgroundVariant.Normal, sigma, tint, 0, 0);
        }

        return FromBand(blurStrength switch
        {
            BlurStrength.Light => Light(),
            BlurStrength.Middle => Middle(),
            _ => Heavy()
        });
    }

    private static string ApplyTreatment(string chain, Treatment treatment)
    {
        if (treatment.BlurSigma.HasValue)
        {
            var sigma = treatment.BlurSigma.Value;
            var steps = sigma <= 5 ? 1 : sigma <= 8 ? 2 : 3;
            chain += $",gblur=sigma={sigma}:steps={steps}";
        }

        return treatment.Variant switch
        {
            BackgroundVariant.GradientTint => chain +
                $",drawbox=x=0:y=0:w={OverlayRenderer.Width}:h={OverlayRenderer.Height * 11 / 20}:t=fill:color=black@{treatment.TintOpacity:0.00}" +
                $",drawbox=x=0:y={OverlayRenderer.Height * 11 / 20}:w={OverlayRenderer.Width}:h={OverlayRenderer.Height - OverlayRenderer.Height * 11 / 20}:t=fill:color=black@{treatment.SecondaryTintOpacity:0.00}",
            BackgroundVariant.CardOverlay => chain +
                $",drawbox=t=fill:color=black@{treatment.TintOpacity:0.00}" +
                $",drawbox=x=70:y=240:w={OverlayRenderer.Width - 140}:h={OverlayRenderer.Height - 480}:t=fill:color=black@{treatment.CardOpacity:0.00}",
            _ => chain + $",drawbox=t=fill:color=black@{treatment.TintOpacity:0.00}"
        };
    }

    private static (int ExitCode, string Output) RunProcess(string fileName, IEnumerable<string> args, CancellationToken cancellationToken)
    {
        var startInfo = new ProcessStartInfo(fileName)
        {
            UseShellExecute = false,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            CreateNoWindow = true
        };
        foreach (var arg in args)
        {
            startInfo.ArgumentList.Add(arg);
        }

        using var process = Process.Start(startInfo) ?? throw new InvalidOperationException($"Could not launch {fileName}");
        var output = new StringBuilder();
        process.OutputDataReceived += (_, e) => { if (e.Data is not null) output.AppendLine(e.Data); };
        process.ErrorDataReceived += (_, e) => { if (e.Data is not null) output.AppendLine(e.Data); };
        process.BeginOutputReadLine();
        process.BeginErrorReadLine();
        while (!process.WaitForExit(200))
        {
            if (!cancellationToken.IsCancellationRequested)
            {
                continue;
            }

            try { process.Kill(entireProcessTree: true); } catch { }
            cancellationToken.ThrowIfCancellationRequested();
        }

        return (process.ExitCode, output.ToString());
    }
}

internal sealed class NaturalStringComparer : IComparer<string>
{
    public int Compare(string? x, string? y)
    {
        x ??= "";
        y ??= "";
        var xParts = Regex.Matches(x, @"\d+|\D+").Select(m => m.Value).ToArray();
        var yParts = Regex.Matches(y, @"\d+|\D+").Select(m => m.Value).ToArray();
        for (var i = 0; i < Math.Min(xParts.Length, yParts.Length); i++)
        {
            var cmp = ulong.TryParse(xParts[i], out var xn) && ulong.TryParse(yParts[i], out var yn)
                ? xn.CompareTo(yn)
                : string.Compare(xParts[i], yParts[i], StringComparison.OrdinalIgnoreCase);
            if (cmp != 0) return cmp;
        }

        return xParts.Length.CompareTo(yParts.Length);
    }
}

internal static class Cli
{
    public static int Run(string[] args)
    {
        var options = Parse(args);
        Console.WriteLine("--------------------------------------------------");
        Console.WriteLine("        REEL FORGE C# - INITIALIZING");
        Console.WriteLine("--------------------------------------------------");
        var summary = Renderer.RenderSource(options, CancellationToken.None, Console.WriteLine);
        return summary.FullySuccessful ? 0 : 2;
    }

    private static RenderOptions Parse(string[] args)
    {
        var map = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
        for (var i = 0; i < args.Length; i++)
        {
            if (!args[i].StartsWith("--")) continue;
            var key = args[i][2..];
            if (i + 1 < args.Length && !args[i + 1].StartsWith("--"))
            {
                map[key] = args[++i];
            }
            else
            {
                map[key] = "true";
            }
        }

        if (!map.TryGetValue("script", out var script))
        {
            throw new InvalidOperationException("--script is required for CLI mode");
        }

        return new RenderOptions
        {
            ScriptPath = script,
            VideosFolder = map.GetValueOrDefault("videos", "input/videos"),
            MusicFolder = map.GetValueOrDefault("music", "input/music"),
            OutputFolder = map.GetValueOrDefault("output", "output/videos"),
            OverlayFolder = map.GetValueOrDefault("overlays", "output/overlays"),
            Duration = float.TryParse(map.GetValueOrDefault("duration", Renderer.DefaultReelDuration.ToString()), out var d) ? d : Renderer.DefaultReelDuration,
            Workers = map.GetValueOrDefault("workers", "auto"),
            Blur = BlurStrengthExtensions.Parse(map.GetValueOrDefault("blur", "none")),
            ErrorLogPath = map.GetValueOrDefault("error-log"),
            TimingLogPath = map.GetValueOrDefault("timing-log")
        };
    }
}

#if WINDOWS
internal sealed class SavedConfig
{
    [JsonPropertyName("video_folder")]
    public string VideoFolder { get; set; } = "input/videos";
    [JsonPropertyName("music_folder")]
    public string MusicFolder { get; set; } = "input/music";
    [JsonPropertyName("output_folder")]
    public string OutputFolder { get; set; } = "output/videos";
    [JsonPropertyName("overlay_folder")]
    public string OverlayFolder { get; set; } = "output/overlays";
    [JsonPropertyName("error_log_path")]
    public string ErrorLogPath { get; set; } = "";
    [JsonPropertyName("timing_log_path")]
    public string TimingLogPath { get; set; } = "";
    [JsonPropertyName("script_source")]
    public string ScriptSource { get; set; } = "";
    [JsonPropertyName("duration")]
    public string Duration { get; set; } = "12";
    [JsonPropertyName("workers")]
    public string Workers { get; set; } = "auto";
    [JsonPropertyName("manual_workers")]
    public bool ManualWorkers { get; set; }
    [JsonPropertyName("blur_strength")]
    public string BlurStrength { get; set; } = "none";
    [JsonPropertyName("script_text")]
    public string ScriptText { get; set; } = "TITLE:Fast C# UI\nPerfect native execution.\nSharp rendering performance.\nCTA:Create, build, succeed.";
}

internal sealed class ReelForgeForm : Form
{
    private readonly TextBox _videoFolder = new();
    private readonly TextBox _musicFolder = new();
    private readonly TextBox _outputFolder = new();
    private readonly TextBox _overlayFolder = new();
    private readonly TextBox _errorLogPath = new();
    private readonly TextBox _timingLogPath = new();
    private readonly TextBox _duration = new();
    private readonly TextBox _workers = new();
    private readonly CheckBox _manualWorkers = new() { Text = "Manual" };
    private readonly ComboBox _blur = new() { DropDownStyle = ComboBoxStyle.DropDownList };
    private readonly TextBox _scriptSource = new();
    private readonly TextBox _scriptText = new() { Multiline = true, ScrollBars = ScrollBars.Vertical, WordWrap = true, Font = new Font("Consolas", 10) };
    private readonly TextBox _log = new() { Multiline = true, ScrollBars = ScrollBars.Vertical, ReadOnly = true, Font = new Font("Consolas", 9), Dock = DockStyle.Fill };
    private readonly Label _status = new() { Text = "Ready", AutoSize = true, Font = new Font("Segoe UI", 10, FontStyle.Bold), ForeColor = Color.FromArgb(0, 180, 100) };
    private readonly ProgressBar _progress = new() { Minimum = 0, Maximum = 1 };
    private readonly Label _progressText = new() { Text = "", AutoSize = true };
    private readonly PictureBox _preview = new() { SizeMode = PictureBoxSizeMode.Zoom, BackColor = Color.Black, Dock = DockStyle.Fill };
    private CancellationTokenSource? _renderCts;
    private string _lastManualWorkers = "4";

    public ReelForgeForm()
    {
        Text = "Reel Forge C# - Native Video Engine";
        Size = new Size(1120, 780);
        MinimumSize = new Size(980, 700);
        BuildUi();
        LoadState();
    }

    private void BuildUi()
    {
        _blur.Items.AddRange(["none", "light", "middle", "heavy"]);
        _blur.SelectedIndex = 0;

        var root = new TableLayoutPanel { Dock = DockStyle.Fill, ColumnCount = 2, Padding = new Padding(14) };
        root.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 60));
        root.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 40));
        Controls.Add(root);

        var left = new TableLayoutPanel { Dock = DockStyle.Fill, RowCount = 5 };
        left.RowStyles.Add(new RowStyle(SizeType.Absolute, 270));
        left.RowStyles.Add(new RowStyle(SizeType.Absolute, 82));
        left.RowStyles.Add(new RowStyle(SizeType.Percent, 100));
        left.RowStyles.Add(new RowStyle(SizeType.Absolute, 42));
        left.RowStyles.Add(new RowStyle(SizeType.Absolute, 32));
        root.Controls.Add(left, 0, 0);

        var right = new TableLayoutPanel { Dock = DockStyle.Fill, RowCount = 2 };
        right.RowStyles.Add(new RowStyle(SizeType.Percent, 58));
        right.RowStyles.Add(new RowStyle(SizeType.Percent, 42));
        root.Controls.Add(right, 1, 0);

        var folders = Group("Configuration Folders");
        left.Controls.Add(folders, 0, 0);
        var folderGrid = Grid(6, 3);
        folders.Controls.Add(folderGrid);
        AddPathRow(folderGrid, 0, "Videos", _videoFolder, PickFolder);
        AddPathRow(folderGrid, 1, "Music", _musicFolder, PickFolder);
        AddPathRow(folderGrid, 2, "Output", _outputFolder, PickFolder);
        AddPathRow(folderGrid, 3, "Overlays", _overlayFolder, PickFolder);
        AddPathRow(folderGrid, 4, "Error File", _errorLogPath, PickFile);
        AddPathRow(folderGrid, 5, "Timing File", _timingLogPath, PickTimingFile);

        var options = Group("Performance Controls");
        left.Controls.Add(options, 0, 1);
        var optionGrid = Grid(2, 5);
        options.Controls.Add(optionGrid);
        optionGrid.Controls.Add(new Label { Text = "Video Duration", AutoSize = true }, 0, 0);
        optionGrid.Controls.Add(_duration, 1, 0);
        optionGrid.Controls.Add(new Label { Text = "Parallel Threads / auto", AutoSize = true }, 2, 0);
        optionGrid.Controls.Add(_workers, 3, 0);
        optionGrid.Controls.Add(_manualWorkers, 4, 0);
        optionGrid.Controls.Add(new Label { Text = "Blur", AutoSize = true }, 0, 1);
        optionGrid.Controls.Add(_blur, 1, 1);
        _manualWorkers.CheckedChanged += (_, _) => ToggleManualWorkers();

        var scriptGroup = Group("Reel Script Editor");
        left.Controls.Add(scriptGroup, 0, 2);
        var scriptLayout = new TableLayoutPanel { Dock = DockStyle.Fill, RowCount = 2 };
        scriptLayout.RowStyles.Add(new RowStyle(SizeType.Absolute, 36));
        scriptLayout.RowStyles.Add(new RowStyle(SizeType.Percent, 100));
        scriptGroup.Controls.Add(scriptLayout);
        var sourceGrid = Grid(1, 4);
        sourceGrid.Controls.Add(new Label { Text = "Script Source", AutoSize = true }, 0, 0);
        sourceGrid.Controls.Add(_scriptSource, 1, 0);
        AddButton(sourceGrid, "File", 2, 0, LoadScript);
        AddButton(sourceGrid, "Folder", 3, 0, LoadScriptFolder);
        sourceGrid.ColumnStyles[1].Width = 100;
        sourceGrid.ColumnStyles[1].SizeType = SizeType.Percent;
        scriptLayout.Controls.Add(sourceGrid, 0, 0);
        scriptLayout.Controls.Add(_scriptText, 0, 1);

        var buttons = new FlowLayoutPanel { Dock = DockStyle.Fill, FlowDirection = FlowDirection.LeftToRight };
        left.Controls.Add(buttons, 0, 3);
        AddAction(buttons, "Use Editor", () => { _scriptSource.Clear(); SaveState(); });
        AddAction(buttons, "Save Settings", SaveInput);
        AddAction(buttons, "Start C# Render", StartRender);
        AddAction(buttons, "Pause", PauseRender);
        AddAction(buttons, "Resume", ResumeRender);
        AddAction(buttons, "Stop", StopRender);
        AddAction(buttons, "Clear Log", () => { _log.Clear(); _progress.Value = 0; _progressText.Text = ""; });
        buttons.Controls.Add(_status);

        var progressRow = new TableLayoutPanel { Dock = DockStyle.Fill, ColumnCount = 2 };
        progressRow.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100));
        progressRow.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 64));
        progressRow.Controls.Add(_progress, 0, 0);
        progressRow.Controls.Add(_progressText, 1, 0);
        left.Controls.Add(progressRow, 0, 4);

        var previewGroup = Group("Visual Layout Preview");
        previewGroup.Controls.Add(_preview);
        right.Controls.Add(previewGroup, 0, 0);
        var logGroup = Group("C# Backend Logs");
        logGroup.Controls.Add(_log);
        right.Controls.Add(logGroup, 0, 1);
    }

    private static GroupBox Group(string title) => new() { Text = title, Dock = DockStyle.Fill, Padding = new Padding(10) };

    private static TableLayoutPanel Grid(int rows, int columns)
    {
        var grid = new TableLayoutPanel { Dock = DockStyle.Fill, RowCount = rows, ColumnCount = columns };
        for (var i = 0; i < columns; i++) grid.ColumnStyles.Add(new ColumnStyle(i == 1 ? SizeType.Percent : SizeType.AutoSize, i == 1 ? 100 : 0));
        return grid;
    }

    private static void AddPathRow(TableLayoutPanel grid, int row, string label, TextBox textBox, Action<TextBox> picker)
    {
        grid.Controls.Add(new Label { Text = label, AutoSize = true }, 0, row);
        grid.Controls.Add(textBox, 1, row);
        AddButton(grid, "Browse", 2, row, () => picker(textBox));
    }

    private static void AddButton(TableLayoutPanel grid, string text, int col, int row, Action action)
    {
        var button = new Button { Text = text, AutoSize = true };
        button.Click += (_, _) => action();
        grid.Controls.Add(button, col, row);
    }

    private static void AddAction(FlowLayoutPanel panel, string text, Action action)
    {
        var button = new Button { Text = text, AutoSize = true };
        button.Click += (_, _) => action();
        panel.Controls.Add(button);
    }

    private void PickFolder(TextBox target)
    {
        using var dialog = new FolderBrowserDialog { InitialDirectory = Directory.Exists(target.Text) ? target.Text : AppPaths.Root };
        if (dialog.ShowDialog(this) == DialogResult.OK)
        {
            target.Text = dialog.SelectedPath;
            SaveState();
        }
    }

    private void PickFile(TextBox target)
    {
        using var dialog = new SaveFileDialog { InitialDirectory = AppPaths.Root, FileName = string.IsNullOrWhiteSpace(target.Text) ? "failed_scripts.txt" : Path.GetFileName(target.Text), Filter = "Text files|*.txt|Log files|*.log|All files|*.*" };
        if (dialog.ShowDialog(this) == DialogResult.OK)
        {
            target.Text = dialog.FileName;
            SaveState();
        }
    }

    private void PickTimingFile(TextBox target)
    {
        using var dialog = new SaveFileDialog
        {
            InitialDirectory = AppPaths.Root,
            FileName = string.IsNullOrWhiteSpace(target.Text) ? "reel_timing.txt" : Path.GetFileName(target.Text),
            Filter = "Text files|*.txt|Log files|*.log|All files|*.*"
        };
        if (dialog.ShowDialog(this) == DialogResult.OK)
        {
            target.Text = dialog.FileName;
            SaveState();
        }
    }

    private void LoadScript()
    {
        using var dialog = new OpenFileDialog { InitialDirectory = AppPaths.Root, Filter = "Text scripts|*.txt|All files|*.*" };
        if (dialog.ShowDialog(this) != DialogResult.OK) return;
        _scriptSource.Text = dialog.FileName;
        _scriptText.Text = File.ReadAllText(dialog.FileName, Encoding.UTF8);
        SaveState();
    }

    private void LoadScriptFolder()
    {
        using var dialog = new FolderBrowserDialog { InitialDirectory = AppPaths.Root };
        if (dialog.ShowDialog(this) != DialogResult.OK) return;
        _scriptSource.Text = dialog.SelectedPath;
        var files = Directory.EnumerateFiles(dialog.SelectedPath, "*.txt", SearchOption.AllDirectories)
            .OrderBy(path => path, new NaturalStringComparer())
            .ToList();
        _scriptText.Text = files.Count == 0
            ? "Folder mode enabled, but no .txt files were found yet."
            : $"Folder mode enabled.\r\nAll .txt files in this folder will be rendered.\r\n\r\n{string.Join("\r\n", files.Take(20))}{(files.Count > 20 ? $"\r\n... and {files.Count - 20} more file(s)" : "")}";
        SaveState();
    }

    private void SaveInput()
    {
        SaveState();
        _status.Text = "Settings saved";
        Log($"Saved layout configuration to {AppPaths.StatePath}");
    }

    private void StartRender()
    {
        StopRender();
        SaveState();
        if (_manualWorkers.Checked && (!int.TryParse(_workers.Text.Trim(), out var manual) || manual <= 0))
        {
            MessageBox.Show(this, "Manual threads must be a positive whole number.", "Error", MessageBoxButtons.OK, MessageBoxIcon.Error);
            return;
        }

        var source = _scriptSource.Text.Trim();
        if (source.Length == 0)
        {
            var text = _scriptText.Text.Trim();
            if (text.Length == 0)
            {
                MessageBox.Show(this, "Please enter a script layout first.", "Error", MessageBoxButtons.OK, MessageBoxIcon.Error);
                return;
            }

            Directory.CreateDirectory(Path.GetDirectoryName(AppPaths.TempScriptPath)!);
            File.WriteAllText(AppPaths.TempScriptPath, text, Encoding.UTF8);
            source = AppPaths.TempScriptPath;
        }
        else if (!File.Exists(source) && !Directory.Exists(source))
        {
            MessageBox.Show(this, "Selected script file or folder does not exist.", "Error", MessageBoxButtons.OK, MessageBoxIcon.Error);
            return;
        }

        _renderCts = new CancellationTokenSource();
        _status.Text = "Rendering...";
        _progress.Value = 0;
        _progressText.Text = "";
        Log("");
        Log("==================================================");
        Log("LAUNCHING C# REEL FORGE BACKEND");
        Log("==================================================");

        var options = new RenderOptions
        {
            ScriptPath = source,
            VideosFolder = _videoFolder.Text,
            MusicFolder = _musicFolder.Text,
            OutputFolder = _outputFolder.Text,
            OverlayFolder = _overlayFolder.Text,
            Duration = float.TryParse(_duration.Text, out var d) ? d : Renderer.DefaultReelDuration,
            Workers = NormalizedWorkers(),
            Blur = BlurStrengthExtensions.Parse(_blur.Text),
            ErrorLogPath = string.IsNullOrWhiteSpace(_errorLogPath.Text) ? null : _errorLogPath.Text,
            TimingLogPath = string.IsNullOrWhiteSpace(_timingLogPath.Text) ? null : _timingLogPath.Text
        };

        Task.Run(() =>
        {
            try
            {
                Renderer.RenderSource(options, _renderCts.Token, line => BeginInvoke((Action)(() => HandleBackendLine(line))));
                BeginInvoke((Action)(() =>
                {
                    _status.Text = _renderCts.IsCancellationRequested ? "Stopped" : "Done";
                    Log(_renderCts.IsCancellationRequested ? "Render stopped." : "Rendering successfully completed!");
                    UpdatePreview();
                }));
            }
            catch (OperationCanceledException)
            {
                BeginInvoke((Action)(() => { _status.Text = "Stopped"; Log("Render stopped."); }));
            }
            catch (Exception ex)
            {
                BeginInvoke((Action)(() => { _status.Text = "Failed"; Log($"Error: {ex.Message}"); }));
            }
            finally
            {
                _renderCts?.Dispose();
                _renderCts = null;
            }
        });
    }

    private void StopRender()
    {
        if (_renderCts is null) return;
        _renderCts.Cancel();
        _status.Text = "Stopping...";
        Log("Stop requested. Active FFmpeg work will be killed as soon as possible.");
    }

    private void PauseRender() => Log("Pause is only available while running external FFmpeg processes; use Stop to cancel the C# render safely.");
    private void ResumeRender() => Log("Resume is not needed unless a process has been paused externally.");

    private void HandleBackendLine(string line)
    {
        Log(line);
        var match = Regex.Match(line, @"Completed batch file\s+(\d+)/(\d+)");
        if (!match.Success) return;
        var current = int.Parse(match.Groups[1].Value);
        var total = Math.Max(1, int.Parse(match.Groups[2].Value));
        _progress.Maximum = total;
        _progress.Value = Math.Min(current, total);
        _progressText.Text = $"{current}/{total}";
    }

    private void Log(string message)
    {
        _log.AppendText(message + Environment.NewLine);
        try
        {
            File.AppendAllText(Path.Combine(AppPaths.Root, "reel_forge.log"), message + Environment.NewLine, Encoding.UTF8);
        }
        catch
        {
            // UI logging should never fail just because the file log is unavailable.
        }
    }

    private void UpdatePreview()
    {
        if (!Directory.Exists(_overlayFolder.Text)) return;
        var png = Directory.EnumerateFiles(_overlayFolder.Text, "*.png", SearchOption.AllDirectories)
            .OrderByDescending(File.GetLastWriteTimeUtc)
            .FirstOrDefault();
        if (png is null) return;

        try
        {
            using var baseImage = new Bitmap(OverlayRenderer.Width, OverlayRenderer.Height, PixelFormat.Format32bppArgb);
            using (var g = Graphics.FromImage(baseImage))
            {
                g.Clear(Color.Black);
                using var darkBrush = new SolidBrush(Color.FromArgb(112, 0, 0, 0));
                g.FillRectangle(darkBrush, 0, 0, baseImage.Width, baseImage.Height);
                using var overlay = Image.FromFile(png);
                g.DrawImage(overlay, 0, 0);
            }

            var preview = new Bitmap(baseImage, new Size(270, 480));
            var old = _preview.Image;
            _preview.Image = preview;
            old?.Dispose();
        }
        catch (Exception ex)
        {
            Log($"Could not build preview image: {ex.Message}");
        }
    }

    private string NormalizedWorkers() => _manualWorkers.Checked ? (string.IsNullOrWhiteSpace(_workers.Text) ? _lastManualWorkers : _workers.Text.Trim()) : "auto";

    private void ToggleManualWorkers()
    {
        if (_manualWorkers.Checked)
        {
            if (int.TryParse(_workers.Text.Trim(), out _)) _lastManualWorkers = _workers.Text.Trim();
            _workers.Text = _lastManualWorkers;
            _workers.Enabled = true;
        }
        else
        {
            if (int.TryParse(_workers.Text.Trim(), out _)) _lastManualWorkers = _workers.Text.Trim();
            _workers.Text = "auto";
            _workers.Enabled = false;
        }
        SaveState();
    }

    private void LoadState()
    {
        var state = new SavedConfig
        {
            VideoFolder = Path.Combine(AppPaths.Root, "input", "videos"),
            MusicFolder = Path.Combine(AppPaths.Root, "input", "music"),
            OutputFolder = Path.Combine(AppPaths.Root, "output", "videos"),
            OverlayFolder = Path.Combine(AppPaths.Root, "output", "overlays")
        };

        if (File.Exists(AppPaths.StatePath))
        {
            try
            {
                state = JsonSerializer.Deserialize<SavedConfig>(File.ReadAllText(AppPaths.StatePath, Encoding.UTF8), JsonOptions()) ?? state;
            }
            catch { }
        }

        _videoFolder.Text = state.VideoFolder;
        _musicFolder.Text = state.MusicFolder;
        _outputFolder.Text = state.OutputFolder;
        _overlayFolder.Text = state.OverlayFolder;
        _errorLogPath.Text = state.ErrorLogPath;
        _timingLogPath.Text = state.TimingLogPath;
        _scriptSource.Text = state.ScriptSource;
        _duration.Text = state.Duration;
        _manualWorkers.Checked = state.ManualWorkers;
        _workers.Text = state.ManualWorkers ? state.Workers : "auto";
        _workers.Enabled = state.ManualWorkers;
        _blur.SelectedItem = _blur.Items.Contains(state.BlurStrength) ? state.BlurStrength : "none";
        _scriptText.Text = state.ScriptText;
    }

    private void SaveState()
    {
        Directory.CreateDirectory(Path.GetDirectoryName(AppPaths.StatePath)!);
        var state = new SavedConfig
        {
            VideoFolder = _videoFolder.Text,
            MusicFolder = _musicFolder.Text,
            OutputFolder = _outputFolder.Text,
            OverlayFolder = _overlayFolder.Text,
            ErrorLogPath = _errorLogPath.Text,
            TimingLogPath = _timingLogPath.Text,
            ScriptSource = _scriptSource.Text,
            Duration = _duration.Text,
            Workers = NormalizedWorkers(),
            ManualWorkers = _manualWorkers.Checked,
            BlurStrength = _blur.Text,
            ScriptText = _scriptText.Text.TrimEnd()
        };
        File.WriteAllText(AppPaths.StatePath, JsonSerializer.Serialize(state, JsonOptions()), Encoding.UTF8);
    }

    private static JsonSerializerOptions JsonOptions() => new() { WriteIndented = true, PropertyNameCaseInsensitive = true };

    protected override void OnFormClosing(FormClosingEventArgs e)
    {
        StopRender();
        SaveState();
        base.OnFormClosing(e);
    }
}
#endif
