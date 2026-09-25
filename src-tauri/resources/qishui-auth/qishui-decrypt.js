'use strict';
// 汽水音乐 VIP 全曲解密（方案二）。
// 用本机 SodaMusic 的 bdms.node 签名请求 track_v2，取 encrypt_info.spade_a，
// 下载加密流并用 AES-128-CTR 逐 sample 解密（对照 Mineradio qishui-audio-decryptor），
// 输出明文 M4A/FLAC 到临时目录，stdout 返回 { ok, path, extension, level, duration }。
// 用法：node qishui-decrypt.cjs <payload.json>

const fs = require('fs');
const https = require('https');
const http = require('http');
const crypto = require('crypto');
const path = require('path');

// ===================== Mp4Box =====================
class Mp4Box {
  constructor({ size, type, offset, data }) { this.size = size; this.type = type; this.offset = offset; this.data = data; }
  isEmpty() { return this.size === 0; }
  static fromBuffer(buffer, offset) {
    if (!Buffer.isBuffer(buffer) || offset + 8 > buffer.length) return new Mp4Box({ size: 0, type: '', offset: 0, data: Buffer.alloc(0) });
    const size = buffer.readUInt32BE(offset);
    const type = buffer.subarray(offset + 4, offset + 8).toString('ascii');
    const end = size >= 8 && offset + size <= buffer.length ? offset + size : buffer.length;
    return new Mp4Box({ size: end - offset, type, offset, data: buffer.subarray(offset + 8, end) });
  }
  static findBox(buffer, boxType, offset = 0, end = buffer.length) {
    let position = offset;
    while (position < end) {
      if (position + 8 > end) break;
      const size = buffer.readUInt32BE(position);
      if (size < 8 || position + size > end) break;
      const type = buffer.subarray(position + 4, position + 8).toString('ascii');
      if (type === boxType) return Mp4Box.fromBuffer(buffer, position);
      position += size;
    }
    return new Mp4Box({ size: 0, type: '', offset: 0, data: Buffer.alloc(0) });
  }
}

// ===================== decrypt utils =====================
function bitCount(value) { let c = value; c = c - ((c >> 1) & 0x55555555); c = (c & 0x33333333) + ((c >> 2) & 0x33333333); return (((c + (c >> 4)) & 0x0f0f0f0f) * 0x01010101) >> 24; }
function decodeBase36(charCode) { if (charCode >= 48 && charCode <= 57) return charCode - 48; if (charCode >= 97 && charCode <= 122) return charCode - 97 + 10; return 0xff; }
function decryptSpadeInner(spadeKey) {
  const result = Buffer.from(spadeKey);
  const working = Buffer.alloc(spadeKey.length + 2);
  working[0] = 0xfa; working[1] = 0x55; spadeKey.copy(working, 2);
  for (let index = 0; index < result.length; index += 1) {
    let value = (spadeKey[index] ^ working[index]) - bitCount(index) - 21;
    while (value < 0) value += 0xff;
    result[index] = value & 0xff;
  }
  return result;
}
function decryptSpade(spadeKeyBytes) {
  if (!Buffer.isBuffer(spadeKeyBytes) || spadeKeyBytes.length < 3) return '';
  const paddingLength = (spadeKeyBytes[0] ^ spadeKeyBytes[1] ^ spadeKeyBytes[2]) - 48;
  if (spadeKeyBytes.length < paddingLength + 2) return '';
  const innerInput = spadeKeyBytes.subarray(1, spadeKeyBytes.length - paddingLength);
  const tempBuffer = decryptSpadeInner(innerInput);
  if (tempBuffer.length === 0) return '';
  const skipBytes = decodeBase36(tempBuffer[0]);
  const decodedMessageLength = spadeKeyBytes.length - paddingLength - 2;
  const endIndex = 1 + decodedMessageLength - skipBytes;
  if (endIndex > tempBuffer.length) return '';
  return tempBuffer.subarray(1, endIndex).toString('utf8');
}
function decryptSpadeA(spadeA) { try { return decryptSpade(Buffer.from(spadeA, 'base64')); } catch { return ''; } }
function hexToBuffer(hex) { if (typeof hex !== 'string' || hex.length % 2 !== 0) throw new Error('Hex length must be even.'); return Buffer.from(hex, 'hex'); }
function aesCtrDecrypt(key, iv, encrypted) { const d = crypto.createDecipheriv('aes-128-ctr', key, iv); return Buffer.concat([d.update(encrypted), d.final()]); }
function parseStsz(data) {
  const sampleSize = data.readUInt32BE(4); const count = data.readUInt32BE(8);
  if (sampleSize !== 0) return Array.from({ length: count }, () => sampleSize);
  const sizes = []; for (let i = 0; i < count; i += 1) sizes.push(data.readUInt32BE(12 + i * 4)); return sizes;
}
function parseStsc(data) {
  const entryCount = data.readUInt32BE(4); const entries = [];
  for (let i = 0; i < entryCount; i += 1) { const base = 8 + i * 12; entries.push({ firstChunk: data.readUInt32BE(base), samplesPerChunk: data.readUInt32BE(base + 4), id: data.readUInt32BE(base + 8) }); }
  return entries;
}
function parseSenc(data) {
  const count = data.readUInt32BE(4); const ivs = []; let position = 8;
  for (let i = 0; i < count; i += 1) { const iv = Buffer.alloc(16); data.copy(iv, 0, position, position + 8); ivs.push(iv); position += 8; }
  return ivs;
}
function scanForFlacMetadata(stsdData) {
  const marker = Buffer.from([0x64, 0x66, 0x4c, 0x61]);
  const index = stsdData.indexOf(marker);
  if (index === -1 || index < 4) return Buffer.alloc(0);
  const boxSize = stsdData.readUInt32BE(index - 4);
  const contentStart = index + 4;
  const contentEnd = Math.min(index - 4 + boxSize, stsdData.length);
  if (contentEnd <= contentStart) return Buffer.alloc(0);
  return stsdData.subarray(contentStart, contentEnd);
}
function replaceEncaWithMp4a(buffer, searchStart, searchEnd) {
  const target = Buffer.from('enca'); const replacement = Buffer.from('mp4a');
  for (let index = searchStart; index + 4 <= searchEnd; index += 1) { if (buffer.subarray(index, index + 4).equals(target)) { replacement.copy(buffer, index); break; } }
}

// ===================== TrackDecryptor =====================
class TrackDecryptor {
  resolveKey(spadeA) {
    if (!spadeA) throw new Error('spade_a is required.');
    const isHex = /^[0-9a-fA-F]+$/.test(spadeA);
    const keyHex = isHex ? spadeA : decryptSpadeA(spadeA);
    if (!keyHex) throw new Error('Failed to resolve key from spade_a.');
    return hexToBuffer(keyHex);
  }
  decryptSampleList({ fileBuffer, key, sampleSizes, ivs, mdatOffset }) {
    const out = []; let off = mdatOffset + 8;
    for (let i = 0; i < sampleSizes.length; i += 1) {
      const size = sampleSizes[i]; const iv = ivs[i];
      if (!iv) throw new Error('Missing IV for sample ' + i);
      out.push(aesCtrDecrypt(key, iv, fileBuffer.subarray(off, off + size)));
      off += size;
    }
    return out;
  }
  buildFlacFile(flacMetadata, decryptedSamples) {
    const body = flacMetadata.length > 4 ? flacMetadata.subarray(4) : flacMetadata;
    return Buffer.concat([Buffer.from('fLaC'), body, ...decryptedSamples]);
  }
  buildM4aFile(fileBuffer, decryptedSamples, mdat, stsd) {
    const output = Buffer.from(fileBuffer); let wp = mdat.offset + 8;
    for (const s of decryptedSamples) { s.copy(output, wp); wp += s.length; }
    replaceEncaWithMp4a(output, stsd.offset, stsd.offset + stsd.size);
    return output;
  }
  decrypt({ encryptedBuffer, spadeA }) {
    if (!Buffer.isBuffer(encryptedBuffer) || encryptedBuffer.length === 0) throw new Error('empty buffer');
    const key = this.resolveKey(spadeA);
    const moov = Mp4Box.findBox(encryptedBuffer, 'moov');
    if (moov.isEmpty()) throw new Error("'moov' not found");
    const trak = Mp4Box.findBox(encryptedBuffer, 'trak', moov.offset + 8, moov.offset + moov.size);
    const mdia = Mp4Box.findBox(encryptedBuffer, 'mdia', trak.offset + 8, trak.offset + trak.size);
    const minf = Mp4Box.findBox(encryptedBuffer, 'minf', mdia.offset + 8, mdia.offset + mdia.size);
    const stbl = Mp4Box.findBox(encryptedBuffer, 'stbl', minf.offset + 8, minf.offset + minf.size);
    const stsd = Mp4Box.findBox(encryptedBuffer, 'stsd', stbl.offset + 8, stbl.offset + stbl.size);
    const stsz = Mp4Box.findBox(encryptedBuffer, 'stsz', stbl.offset + 8, stbl.offset + stbl.size);
    const stsc = Mp4Box.findBox(encryptedBuffer, 'stsc', stbl.offset + 8, stbl.offset + stbl.size);
    const stco = Mp4Box.findBox(encryptedBuffer, 'stco', stbl.offset + 8, stbl.offset + stbl.size);
    let senc = Mp4Box.findBox(encryptedBuffer, 'senc', moov.offset + 8, moov.offset + moov.size);
    if (senc.isEmpty()) senc = Mp4Box.findBox(encryptedBuffer, 'senc', stbl.offset + 8, stbl.offset + stbl.size);
    if (senc.isEmpty()) throw new Error("'senc' not found");
    const mdat = Mp4Box.findBox(encryptedBuffer, 'mdat');
    if (mdat.isEmpty()) throw new Error("'mdat' not found");
    const flacMetadata = scanForFlacMetadata(stsd.data);
    const isFlac = flacMetadata.length > 0;
    const sampleSizes = parseStsz(stsz.data);
    const ivs = parseSenc(senc.data);
    if (sampleSizes.length !== ivs.length) throw new Error('sample/iv count mismatch ' + sampleSizes.length + '/' + ivs.length);
    const decryptedSamples = this.decryptSampleList({ fileBuffer: encryptedBuffer, key, sampleSizes, ivs, mdatOffset: mdat.offset });
    const outputBuffer = isFlac ? this.buildFlacFile(flacMetadata, decryptedSamples) : this.buildM4aFile(encryptedBuffer, decryptedSamples, mdat, stsd);
    return { buffer: outputBuffer, extension: isFlac ? '.flac' : '.m4a' };
  }
}

// ===================== http helpers =====================
function requestBuffer(url, opts, body) {
  return new Promise((resolve, reject) => {
    const u = new URL(url);
    const lib = u.protocol === 'http:' ? http : https;
    const req = lib.request(u, { method: (opts && opts.method) || 'GET', headers: (opts && opts.headers) || {} }, (res) => {
      if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
        res.resume();
        return requestBuffer(new URL(res.headers.location, url).toString(), opts, body).then(resolve, reject);
      }
      const chunks = [];
      res.on('data', (d) => chunks.push(d));
      res.on('end', () => resolve({ status: res.statusCode, buffer: Buffer.concat(chunks) }));
    });
    req.on('error', reject);
    req.setTimeout(30000, () => req.destroy(new Error('timeout')));
    if (body != null) req.write(body);
    req.end();
  });
}

function signHeaders(bdms, url, base) {
  const headerLines = [];
  for (const k of Object.keys(base)) headerLines.push(k + '\r\n' + base[k]);
  const sig = String(bdms.generateHttpSignatureHeaders(url, headerLines.join('\r\n'))).split('\r\n').filter((t) => t.trim());
  const headers = Object.assign({}, base);
  for (let i = 0; i < Math.floor(sig.length / 2); i++) headers[sig[i * 2]] = sig[i * 2 + 1];
  return headers;
}

async function main() {
  const payload = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'));
  const bdms = require(payload.bdmsPath);
  bdms.init({ deviceId: String(payload.deviceId || '') });
  const headers = signHeaders(bdms, payload.url, payload.headers);
  const body = payload.body == null ? null : String(payload.body);
  const resp = await requestBuffer(payload.url, { method: payload.method || 'POST', headers }, body);
  if (resp.status < 200 || resp.status >= 300) throw new Error('track_v2 HTTP ' + resp.status);
  const trackJson = JSON.parse(resp.buffer.toString('utf8'));
  let vm = trackJson.track_player && trackJson.track_player.video_model;
  if (typeof vm === 'string') vm = JSON.parse(vm);
  const list = (vm && vm.video_list) || [];
  if (!list.length) throw new Error('no video_list');
  const pick = (q) => list.find((v) => v.video_meta && v.video_meta.quality === q);
  const want = String(payload.quality || '').toLowerCase();
  const chosen = ((want.indexOf('lossless') >= 0 || want.indexOf('jymaster') >= 0) ? pick('lossless')
    : want.indexOf('hires') >= 0 ? pick('hi_res')
      : (want.indexOf('higher') >= 0 || want.indexOf('exhigh') >= 0) ? pick('higher') : null)
    || pick('lossless') || pick('hi_res') || pick('highest') || pick('higher') || pick('medium') || list[0];
  if (!chosen || !chosen.main_url) throw new Error('no stream url');
  const enc = chosen.encrypt_info || {};
  const spadeA = enc.spade_a || chosen.play_auth || '';
  const track = trackJson.track || {};
  const level = (chosen.video_meta && chosen.video_meta.quality) || 'unknown';
  const prefix = 'qishui-' + (track.id || '') + '-' + level;
  // 缓存命中：同一首同一音质之前已解密过（重播、队列循环、切回上一首），
  // 直接复用明文文件 —— 整首下载 + 解密是首播耗时的全部来源，能省就省。
  if (track.id) {
    let hit = '';
    try {
      hit = fs.readdirSync(payload.outDir).find((f) => {
        if (!f.startsWith(prefix + '.')) return false;
        try { return fs.statSync(path.join(payload.outDir, f)).size > 1024; } catch { return false; }
      }) || '';
    } catch {}
    if (hit) {
      process.stdout.write(JSON.stringify({
        ok: true,
        path: path.join(payload.outDir, hit),
        extension: path.extname(hit),
        level,
        duration: track.duration || track.duration_ms || 0,
        cached: true,
      }));
      return;
    }
  }
  const dl = await requestBuffer(chosen.main_url, { headers: { 'User-Agent': 'Mozilla/5.0' } });
  if (dl.status < 200 || dl.status >= 300) throw new Error('media HTTP ' + dl.status);
  let outBuf = dl.buffer;
  let ext = '.m4a';
  if (enc.encrypt && spadeA) {
    const dec = new TrackDecryptor().decrypt({ encryptedBuffer: dl.buffer, spadeA });
    outBuf = dec.buffer; ext = dec.extension;
  }
  const outPath = path.join(payload.outDir, prefix + ext);
  fs.writeFileSync(outPath, outBuf);
  process.stdout.write(JSON.stringify({
    ok: true,
    path: outPath,
    extension: ext,
    level,
    duration: track.duration || track.duration_ms || 0,
  }));
}

main().catch((e) => {
  process.stdout.write(JSON.stringify({ ok: false, error: String((e && e.message) || e) }));
});
