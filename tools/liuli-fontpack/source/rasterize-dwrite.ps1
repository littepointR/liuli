param(
    [Parameter(Mandatory = $true)][string]$FontPath,
    [Parameter(Mandatory = $true)][string]$CharsetPath,
    [Parameter(Mandatory = $true)][int]$TargetPx,
    [Parameter(Mandatory = $true)][int]$Ascent,
    [Parameter(Mandatory = $true)][int]$Descent,
    [Parameter(Mandatory = $true)][int]$LineGap,
    [Parameter(Mandatory = $true)][string]$OutPbm,
    [Parameter(Mandatory = $true)][string]$OutTsv,
    [int]$FaceIndex = 0,
    [int]$AtlasWidth = 1024,
    [string]$BasePbm = '',
    [string]$BaseTsv = ''
)

$ErrorActionPreference = 'Stop'

if (-not (Test-Path $FontPath)) {
    throw "Font not found: $FontPath"
}
if (-not (Test-Path $CharsetPath)) {
    throw "Charset not found: $CharsetPath"
}
if (($BasePbm -ne '') -xor ($BaseTsv -ne '')) {
    throw "Both -BasePbm and -BaseTsv must be specified together"
}

$code = @'
using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;

public static class DWriteRasterizer
{
    [StructLayout(LayoutKind.Sequential)]
    private struct DWRITE_FONT_METRICS
    {
        public ushort designUnitsPerEm;
        public ushort ascent;
        public ushort descent;
        public short lineGap;
        public ushort capHeight;
        public ushort xHeight;
        public short underlinePosition;
        public ushort underlineThickness;
        public short strikethroughPosition;
        public ushort strikethroughThickness;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct DWRITE_GLYPH_METRICS
    {
        public int leftSideBearing;
        public uint advanceWidth;
        public int rightSideBearing;
        public int topSideBearing;
        public uint advanceHeight;
        public int bottomSideBearing;
        public int verticalOriginY;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct DWRITE_GLYPH_RUN
    {
        public IntPtr fontFace;
        public float fontEmSize;
        public uint glyphCount;
        public IntPtr glyphIndices;
        public IntPtr glyphAdvances;
        public IntPtr glyphOffsets;
        public int isSideways;
        public uint bidiLevel;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct RECT
    {
        public int Left;
        public int Top;
        public int Right;
        public int Bottom;
    }

    [UnmanagedFunctionPointer(CallingConvention.StdCall)]
    private delegate int CreateFontFileReferenceFn(
        IntPtr factory,
        [MarshalAs(UnmanagedType.LPWStr)] string filePath,
        IntPtr lastWriteTime,
        out IntPtr fontFile);

    [UnmanagedFunctionPointer(CallingConvention.StdCall)]
    private delegate int AnalyzeFontFileFn(
        IntPtr fontFile,
        out int isSupported,
        out int fontFileType,
        out int fontFaceType,
        out uint numberOfFaces);

    [UnmanagedFunctionPointer(CallingConvention.StdCall)]
    private delegate int CreateFontFaceFn(
        IntPtr factory,
        int fontFaceType,
        uint numberOfFiles,
        [In] IntPtr[] fontFiles,
        uint faceIndex,
        int fontSimulations,
        out IntPtr fontFace);

    [UnmanagedFunctionPointer(CallingConvention.StdCall)]
    private delegate void GetFontFaceMetricsFn(
        IntPtr fontFace,
        out DWRITE_FONT_METRICS metrics);

    [UnmanagedFunctionPointer(CallingConvention.StdCall)]
    private delegate int GetDesignGlyphMetricsFn(
        IntPtr fontFace,
        [In] ushort[] glyphIndices,
        uint glyphCount,
        [Out] DWRITE_GLYPH_METRICS[] glyphMetrics,
        int isSideways);

    [UnmanagedFunctionPointer(CallingConvention.StdCall)]
    private delegate int GetGlyphIndicesFn(
        IntPtr fontFace,
        [In] uint[] codePoints,
        uint codePointCount,
        [Out] ushort[] glyphIndices);

    [UnmanagedFunctionPointer(CallingConvention.StdCall)]
    private delegate int CreateGlyphRunAnalysisFn(
        IntPtr factory,
        ref DWRITE_GLYPH_RUN glyphRun,
        float pixelsPerDip,
        IntPtr transform,
        int renderingMode,
        int measuringMode,
        float baselineOriginX,
        float baselineOriginY,
        out IntPtr analysis);

    [UnmanagedFunctionPointer(CallingConvention.StdCall)]
    private delegate int GetAlphaTextureBoundsFn(
        IntPtr analysis,
        int textureType,
        out RECT bounds);

    [UnmanagedFunctionPointer(CallingConvention.StdCall)]
    private delegate int CreateAlphaTextureFn(
        IntPtr analysis,
        int textureType,
        ref RECT bounds,
        [Out] byte[] alphaValues,
        uint bufferSize);

    [DllImport("dwrite.dll", ExactSpelling = true)]
    private static extern int DWriteCreateFactory(
        int factoryType,
        ref Guid iid,
        out IntPtr factory);

    private static T GetSlot<T>(IntPtr comPtr, int index) where T : class
    {
        IntPtr vtbl = Marshal.ReadIntPtr(comPtr);
        IntPtr fn = Marshal.ReadIntPtr(vtbl, index * IntPtr.Size);
        return Marshal.GetDelegateForFunctionPointer<T>(fn);
    }

    private static void CheckHr(int hr, string op)
    {
        if (hr < 0)
        {
            throw new InvalidOperationException(string.Format(
                CultureInfo.InvariantCulture, "{0} failed with HRESULT 0x{1:X8}", op, hr));
        }
    }

    private static int FloorDiv4(int v)
    {
        return v >= 0 ? (v / 4) : ((v - 3) / 4);
    }

    private sealed class RasterizedGlyph
    {
        public uint Unicode;
        public ushort WidthPx;
        public ushort HeightPx;
        public short BearingX;
        public short BearingY;
        public ushort Advance;
        public bool[] Pixels4x;
    }

    private static void ParseBasePbm(string path, out int width, out int height, out byte[] rawPixels)
    {
        byte[] data = File.ReadAllBytes(path);
        if (data.Length < 4 || data[0] != (byte)'P' || data[1] != (byte)'4')
        {
            throw new InvalidOperationException("Base PBM must be P4 format: " + path);
        }
        int pos = 2;
        Func<string> nextTok = () =>
        {
            while (pos < data.Length)
            {
                if (data[pos] == (byte)'#')
                {
                    while (pos < data.Length && data[pos] != (byte)'\n') pos++;
                }
                else if (data[pos] <= (byte)' ')
                {
                    pos++;
                }
                else
                {
                    break;
                }
            }
            int start = pos;
            while (pos < data.Length && data[pos] > (byte)' ' && data[pos] != (byte)'#')
            {
                pos++;
            }
            if (pos == start) throw new InvalidOperationException("Unexpected EOF in PBM header");
            return Encoding.ASCII.GetString(data, start, pos - start);
        };
        width = int.Parse(nextTok(), CultureInfo.InvariantCulture);
        height = int.Parse(nextTok(), CultureInfo.InvariantCulture);
        if (pos >= data.Length || data[pos] > (byte)' ')
        {
            throw new InvalidOperationException("Missing single whitespace after P4 header");
        }
        pos++;
        int rowBytes = (width + 7) / 8;
        int expected = rowBytes * height;
        if (data.Length - pos != expected)
        {
            throw new InvalidOperationException("P4 byte count mismatch in base PBM");
        }
        rawPixels = new byte[expected];
        Buffer.BlockCopy(data, pos, rawPixels, 0, expected);
    }

    public static void BuildAtlas(
        string fontPath,
        int faceIndex,
        string charsetPath,
        int targetPx,
        int ascent,
        int descent,
        int lineGap,
        int atlasWidth,
        string basePbmPath,
        string baseTsvPath,
        string outPbmPath,
        string outTsvPath)
    {
        if (atlasWidth % 8 != 0 || atlasWidth % 4 != 0)
        {
            throw new ArgumentException("AtlasWidth must be a multiple of 8");
        }

        string charsetText = File.ReadAllText(charsetPath, Encoding.UTF8);
        var wantedSet = new SortedSet<uint>();
        for (int i = 0; i < charsetText.Length; i++)
        {
            char c = charsetText[i];
            if (c == '\r' || c == '\n') continue;
            if (char.IsHighSurrogate(c) && i + 1 < charsetText.Length && char.IsLowSurrogate(charsetText[i + 1]))
            {
                wantedSet.Add((uint)char.ConvertToUtf32(c, charsetText[i + 1]));
                i++;
            }
            else
            {
                wantedSet.Add((uint)c);
            }
        }

        var existingRows = new List<string>();
        var existingSet = new HashSet<uint>();
        int baseWidth = atlasWidth;
        int baseHeight = 0;
        byte[] basePixels = null;
        string headerComment = string.Format(
            CultureInfo.InvariantCulture, "# ascent={0}\tdescent={1}\tline_gap={2}", ascent, descent, lineGap);

        if (!string.IsNullOrEmpty(baseTsvPath))
        {
            ParseBasePbm(basePbmPath, out baseWidth, out baseHeight, out basePixels);
            if (baseWidth != atlasWidth)
            {
                throw new InvalidOperationException("Base PBM width does not match AtlasWidth");
            }
            string[] lines = File.ReadAllLines(baseTsvPath, Encoding.UTF8);
            bool sawHeader = false;
            foreach (string rawLine in lines)
            {
                string line = rawLine.TrimEnd('\r');
                if (string.IsNullOrWhiteSpace(line)) continue;
                if (line.StartsWith("#"))
                {
                    headerComment = line;
                    continue;
                }
                if (!sawHeader && line.StartsWith("unicode_hex"))
                {
                    sawHeader = true;
                    continue;
                }
                string[] cols = line.Split('\t');
                uint u = uint.Parse(cols[0], NumberStyles.HexNumber, CultureInfo.InvariantCulture);
                existingSet.Add(u);
                existingRows.Add(line);
            }
        }

        var toRasterize = new List<uint>();
        foreach (uint u in wantedSet)
        {
            if (!existingSet.Contains(u))
            {
                toRasterize.Add(u);
            }
        }

        var rasterized = new List<RasterizedGlyph>(toRasterize.Count);
        if (toRasterize.Count > 0)
        {
            IntPtr factory = IntPtr.Zero;
            IntPtr fontFile = IntPtr.Zero;
            IntPtr fontFace = IntPtr.Zero;
            try
            {
                Guid iidFactory = new Guid("b859ee5a-d838-4b5b-a2e8-1adc7d93db48");
                CheckHr(DWriteCreateFactory(0, ref iidFactory, out factory), "DWriteCreateFactory");

                var createFileRef = GetSlot<CreateFontFileReferenceFn>(factory, 7);
                CheckHr(createFileRef(factory, Path.GetFullPath(fontPath), IntPtr.Zero, out fontFile), "CreateFontFileReference");

                var analyze = GetSlot<AnalyzeFontFileFn>(fontFile, 5);
                int isSupported, fileType, faceType;
                uint numFaces;
                CheckHr(analyze(fontFile, out isSupported, out fileType, out faceType, out numFaces), "Analyze");
                if (isSupported == 0 || (uint)faceIndex >= numFaces)
                {
                    throw new InvalidOperationException("Unsupported font file or face index out of range");
                }

                var createFace = GetSlot<CreateFontFaceFn>(factory, 9);
                CheckHr(createFace(factory, faceType, 1, new[] { fontFile }, (uint)faceIndex, 0, out fontFace), "CreateFontFace");

                var getFaceMetrics = GetSlot<GetFontFaceMetricsFn>(fontFace, 8);
                DWRITE_FONT_METRICS fm;
                getFaceMetrics(fontFace, out fm);

                var getIndices = GetSlot<GetGlyphIndicesFn>(fontFace, 11);
                uint[] cpArray = toRasterize.ToArray();
                ushort[] glyphIds = new ushort[cpArray.Length];
                CheckHr(getIndices(fontFace, cpArray, (uint)cpArray.Length, glyphIds), "GetGlyphIndices");

                for (int i = 0; i < cpArray.Length; i++)
                {
                    if (glyphIds[i] == 0 && cpArray[i] != 0)
                    {
                        throw new InvalidOperationException(string.Format(
                            CultureInfo.InvariantCulture,
                            "Font missing glyph (index 0) for U+{0:X4} ('{1}')",
                            cpArray[i],
                            char.ConvertFromUtf32((int)cpArray[i])));
                    }
                }

                var getDesignMetrics = GetSlot<GetDesignGlyphMetricsFn>(fontFace, 10);
                var designMetrics = new DWRITE_GLYPH_METRICS[glyphIds.Length];
                CheckHr(getDesignMetrics(fontFace, glyphIds, (uint)glyphIds.Length, designMetrics, 0), "GetDesignGlyphMetrics");

                var createAnalysis = GetSlot<CreateGlyphRunAnalysisFn>(factory, 23);
                float emSize = targetPx * 4.0f;
                const int DWRITE_RENDERING_MODE_ALIASED = 1;
                const int DWRITE_MEASURING_MODE_NATURAL = 0;
                const int DWRITE_TEXTURE_ALIASED_1x1 = 0;

                for (int i = 0; i < cpArray.Length; i++)
                {
                    uint cp = cpArray[i];
                    ushort gid = glyphIds[i];
                    int adv = (int)Math.Round(
                        designMetrics[i].advanceWidth * ((double)targetPx / fm.designUnitsPerEm),
                        MidpointRounding.AwayFromZero);
                    if (adv < 0 || adv > ushort.MaxValue) adv = targetPx;

                    ushort[] singleGid = new[] { gid };
                    GCHandle handle = GCHandle.Alloc(singleGid, GCHandleType.Pinned);
                    IntPtr analysis = IntPtr.Zero;
                    try
                    {
                        var run = new DWRITE_GLYPH_RUN
                        {
                            fontFace = fontFace,
                            fontEmSize = emSize,
                            glyphCount = 1,
                            glyphIndices = handle.AddrOfPinnedObject(),
                            glyphAdvances = IntPtr.Zero,
                            glyphOffsets = IntPtr.Zero,
                            isSideways = 0,
                            bidiLevel = 0,
                        };
                        CheckHr(
                            createAnalysis(
                                factory,
                                ref run,
                                1.0f,
                                IntPtr.Zero,
                                DWRITE_RENDERING_MODE_ALIASED,
                                DWRITE_MEASURING_MODE_NATURAL,
                                0.0f,
                                0.0f,
                                out analysis),
                            "CreateGlyphRunAnalysis");

                        var getBounds = GetSlot<GetAlphaTextureBoundsFn>(analysis, 3);
                        RECT bounds;
                        CheckHr(getBounds(analysis, DWRITE_TEXTURE_ALIASED_1x1, out bounds), "GetAlphaTextureBounds");

                        int bw = bounds.Right - bounds.Left;
                        int bh = bounds.Bottom - bounds.Top;
                        if (bw <= 0 || bh <= 0)
                        {
                            rasterized.Add(new RasterizedGlyph
                            {
                                Unicode = cp,
                                WidthPx = 0,
                                HeightPx = 0,
                                BearingX = 0,
                                BearingY = 0,
                                Advance = (ushort)adv,
                                Pixels4x = Array.Empty<bool>(),
                            });
                            continue;
                        }

                        byte[] alpha = new byte[bw * bh];
                        var createTex = GetSlot<CreateAlphaTextureFn>(analysis, 4);
                        CheckHr(createTex(analysis, DWRITE_TEXTURE_ALIASED_1x1, ref bounds, alpha, (uint)alpha.Length), "CreateAlphaTexture");

                        int minTx = int.MaxValue;
                        int maxTx = int.MinValue;
                        int minTy = int.MaxValue;
                        int maxTy = int.MinValue;

                        for (int ry = 0; ry < bh; ry++)
                        {
                            int py = bounds.Top + ry;
                            int ty = FloorDiv4(py);
                            for (int rx = 0; rx < bw; rx++)
                            {
                                if (alpha[ry * bw + rx] < 128) continue;
                                int px = bounds.Left + rx;
                                int tx = FloorDiv4(px);
                                if (tx < minTx) minTx = tx;
                                if (tx > maxTx) maxTx = tx;
                                if (ty < minTy) minTy = ty;
                                if (ty > maxTy) maxTy = ty;
                            }
                        }

                        if (minTx > maxTx || minTy > maxTy)
                        {
                            rasterized.Add(new RasterizedGlyph
                            {
                                Unicode = cp,
                                WidthPx = 0,
                                HeightPx = 0,
                                BearingX = 0,
                                BearingY = 0,
                                Advance = (ushort)adv,
                                Pixels4x = Array.Empty<bool>(),
                            });
                            continue;
                        }

                        int targetW = maxTx - minTx + 1;
                        int targetH = maxTy - minTy + 1;
                        int pxW = targetW * 4;
                        int pxH = targetH * 4;
                        bool[] cell4x = new bool[pxW * pxH];
                        int ox = minTx * 4;
                        int oy = minTy * 4;
                        for (int ry = 0; ry < bh; ry++)
                        {
                            int py = bounds.Top + ry;
                            int dy = py - oy;
                            if ((uint)dy >= (uint)pxH) continue;
                            for (int rx = 0; rx < bw; rx++)
                            {
                                if (alpha[ry * bw + rx] < 128) continue;
                                int px = bounds.Left + rx;
                                int dx = px - ox;
                                if ((uint)dx < (uint)pxW)
                                {
                                    cell4x[dy * pxW + dx] = true;
                                }
                            }
                        }

                        rasterized.Add(new RasterizedGlyph
                        {
                            Unicode = cp,
                            WidthPx = (ushort)targetW,
                            HeightPx = (ushort)targetH,
                            BearingX = (short)minTx,
                            BearingY = (short)(-minTy),
                            Advance = (ushort)adv,
                            Pixels4x = cell4x,
                        });
                    }
                    finally
                    {
                        if (analysis != IntPtr.Zero) Marshal.Release(analysis);
                        handle.Free();
                    }
                }
            }
            finally
            {
                if (fontFace != IntPtr.Zero) Marshal.Release(fontFace);
                if (fontFile != IntPtr.Zero) Marshal.Release(fontFile);
                if (factory != IntPtr.Zero) Marshal.Release(factory);
            }
        }

        // Pack rasterized glyphs onto 4px-aligned shelves starting at baseHeight
        int curX = 0;
        int curY = baseHeight;
        int shelfH = 0;
        var placements = new List<Tuple<RasterizedGlyph, int, int>>(rasterized.Count);
        foreach (var g in rasterized)
        {
            int gw4 = g.WidthPx * 4;
            int gh4 = g.HeightPx * 4;
            if (gw4 == 0 || gh4 == 0)
            {
                placements.Add(Tuple.Create(g, 0, curY));
                continue;
            }
            if (gw4 > atlasWidth)
            {
                throw new InvalidOperationException("Single glyph 4x width exceeds AtlasWidth");
            }
            if (curX + gw4 > atlasWidth)
            {
                curX = 0;
                curY += shelfH;
                shelfH = 0;
            }
            placements.Add(Tuple.Create(g, curX, curY));
            curX += gw4 + 4;
            if (gh4 + 4 > shelfH)
            {
                shelfH = gh4 + 4;
            }
        }

        int totalHeight = curY + shelfH;
        if (totalHeight <= baseHeight) totalHeight = Math.Max(baseHeight, 4);
        int rowBytes = atlasWidth / 8;
        byte[] outPixels = new byte[rowBytes * totalHeight];
        if (basePixels != null)
        {
            Buffer.BlockCopy(basePixels, 0, outPixels, 0, basePixels.Length);
        }

        foreach (var p in placements)
        {
            var g = p.Item1;
            int cx = p.Item2;
            int cy = p.Item3;
            int gw4 = g.WidthPx * 4;
            int gh4 = g.HeightPx * 4;
            for (int dy = 0; dy < gh4; dy++)
            {
                int rowOff = (cy + dy) * rowBytes;
                for (int dx = 0; dx < gw4; dx++)
                {
                    if (g.Pixels4x[dy * gw4 + dx])
                    {
                        int x = cx + dx;
                        outPixels[rowOff + (x >> 3)] |= (byte)(0x80 >> (x & 7));
                    }
                }
            }
        }

        Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(outPbmPath)));
        using (var fs = new FileStream(outPbmPath, FileMode.Create, FileAccess.Write))
        {
            byte[] hdr = Encoding.ASCII.GetBytes(
                string.Format(CultureInfo.InvariantCulture, "P4\n{0} {1}\n", atlasWidth, totalHeight));
            fs.Write(hdr, 0, hdr.Length);
            fs.Write(outPixels, 0, outPixels.Length);
        }

        var tsvLines = new List<string>(existingRows.Count + placements.Count + 2);
        tsvLines.Add(headerComment);
        tsvLines.Add("unicode_hex\tcell_x\tcell_y\twidth_px\theight_px\tbearing_x\tbearing_y\tadvance");
        foreach (string row in existingRows)
        {
            tsvLines.Add(row);
        }
        foreach (var p in placements)
        {
            var g = p.Item1;
            tsvLines.Add(string.Format(
                CultureInfo.InvariantCulture,
                "{0:X4}\t{1}\t{2}\t{3}\t{4}\t{5}\t{6}\t{7}",
                g.Unicode,
                p.Item2,
                p.Item3,
                g.WidthPx,
                g.HeightPx,
                g.BearingX,
                g.BearingY,
                g.Advance));
        }

        Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(outTsvPath)));
        File.WriteAllText(outTsvPath, string.Join("\n", tsvLines) + "\n", new UTF8Encoding(false));
    }
}
'@

Add-Type -TypeDefinition $code -Language CSharp

[DWriteRasterizer]::BuildAtlas(
    $FontPath,
    $FaceIndex,
    $CharsetPath,
    $TargetPx,
    $Ascent,
    $Descent,
    $LineGap,
    $AtlasWidth,
    $BasePbm,
    $BaseTsv,
    $OutPbm,
    $OutTsv
)

Write-Host ("Rasterized {0} + {1} from {2} @ {3}px" -f $OutPbm, $OutTsv, $FontPath, $TargetPx)
