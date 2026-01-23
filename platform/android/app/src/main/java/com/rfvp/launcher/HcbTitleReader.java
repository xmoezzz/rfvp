package com.rfvp.launcher;

import androidx.annotation.Nullable;

import java.io.File;
import java.io.RandomAccessFile;
import java.nio.ByteBuffer;
import java.nio.CharBuffer;
import java.nio.charset.Charset;
import java.nio.charset.CharsetDecoder;
import java.nio.charset.CodingErrorAction;

public final class HcbTitleReader {

    private HcbTitleReader() {}

    @Nullable
    public static String readTitle(File hcbFile) throws Exception {
        try (RandomAccessFile raf = new RandomAccessFile(hcbFile, "r")) {
            long len = raf.length();
            if (len < 16) return null;

            long sysDescOff = readU32LE(raf, 0);
            if (sysDescOff <= 0 || sysDescOff >= len) return null;

            long off = sysDescOff;
            // entry_point u32
            off += 4;
            // non_volatile_global_count u16
            off += 2;
            // volatile_global_count u16
            off += 2;
            // game_mode u16
            off += 2;

            int titleLen = readU8(raf, off);
            off += 1;

            if (titleLen <= 0 || off + titleLen > len) return null;

            byte[] titleBytes = new byte[titleLen];
            raf.seek(off);
            raf.readFully(titleBytes);

            int cut = 0;
            while (cut < titleBytes.length && titleBytes[cut] != 0) cut++;
            byte[] raw = new byte[cut];
            System.arraycopy(titleBytes, 0, raw, 0, cut);

            return decodeBestEffort(raw);
        }
    }

    private static long readU32LE(RandomAccessFile raf, long off) throws Exception {
        raf.seek(off);
        int b0 = raf.read();
        int b1 = raf.read();
        int b2 = raf.read();
        int b3 = raf.read();
        if ((b0 | b1 | b2 | b3) < 0) throw new IllegalStateException("EOF");
        return ((long)b0) | ((long)b1 << 8) | ((long)b2 << 16) | ((long)b3 << 24);
    }

    private static int readU8(RandomAccessFile raf, long off) throws Exception {
        raf.seek(off);
        int b = raf.read();
        if (b < 0) throw new IllegalStateException("EOF");
        return b & 0xFF;
    }

    private static String decodeBestEffort(byte[] raw) {
        // Priority: Shift_JIS (typical), then GB18030/GBK, then UTF-8.
        String[] charsets = new String[] { "Shift_JIS", "GB18030", "GBK", "UTF-8" };

        for (String cs : charsets) {
            String s = tryDecodeStrict(raw, cs);
            if (s != null && isReasonable(s)) {
                return s.trim();
            }
        }

        // Fallback with replacement
        try {
            return new String(raw, Charset.forName("Shift_JIS")).trim();
        } catch (Throwable ignored) {
            return new String(raw).trim();
        }
    }

    private static boolean isReasonable(String s) {
        if (s == null) return false;
        if (s.isEmpty()) return false;
        int bad = 0;
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            if (c == '\uFFFD') bad++;
            if (Character.isISOControl(c) && !Character.isWhitespace(c)) bad++;
        }
        // Very simple heuristic: avoid strings with too many replacement/control chars.
        return bad * 4 < s.length();
    }

    @Nullable
    private static String tryDecodeStrict(byte[] raw, String cs) {
        try {
            CharsetDecoder dec = Charset.forName(cs).newDecoder();
            dec.onMalformedInput(CodingErrorAction.REPORT);
            dec.onUnmappableCharacter(CodingErrorAction.REPORT);
            CharBuffer out = dec.decode(ByteBuffer.wrap(raw));
            return out.toString();
        } catch (Throwable t) {
            return null;
        }
    }
}
