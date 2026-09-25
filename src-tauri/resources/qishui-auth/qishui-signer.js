'use strict';
// 汽水音乐官方签名桥（方案二）。
// 由本机 SodaMusic 客户端自带的 bdms.node + metasecml.dll 生成 X-Helios/X-Medusa 签名，
// 再用 https 发起请求，把响应写回 stdout（JSON: {status, body} / {status:0, error}）。
// 用法：<SodaMusic.exe>（设 ELECTRON_RUN_AS_NODE=1） qishui-signer.js <payload.json>
// payload.json: { bdmsPath, deviceId, url, method, headers:{k:v}, body }

const fs = require('fs');
const https = require('https');
const http = require('http');

function main() {
  const payload = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'));
  const bdms = require(payload.bdmsPath);
  if (bdms && typeof bdms.init === 'function') {
    bdms.init({ deviceId: String(payload.deviceId || '') });
  }
  const base = payload.headers || {};
  const headerLines = [];
  for (const key of Object.keys(base)) {
    headerLines.push(key + '\r\n' + base[key]);
  }
  const sig = String(bdms.generateHttpSignatureHeaders(payload.url, headerLines.join('\r\n')))
    .split('\r\n')
    .filter(function (t) { return t.trim(); });
  const headers = Object.assign({}, base);
  for (let i = 0; i < Math.floor(sig.length / 2); i++) {
    headers[sig[i * 2]] = sig[i * 2 + 1];
  }

  const u = new URL(payload.url);
  const lib = u.protocol === 'http:' ? http : https;
  const body = payload.body == null ? null : String(payload.body);
  const req = lib.request(u, { method: payload.method || 'GET', headers: headers }, function (res) {
    const chunks = [];
    res.on('data', function (d) { chunks.push(d); });
    res.on('end', function () {
      process.stdout.write(JSON.stringify({ status: res.statusCode, body: Buffer.concat(chunks).toString('utf8') }));
    });
  });
  req.on('error', function (e) {
    process.stdout.write(JSON.stringify({ status: 0, error: String((e && e.message) || e) }));
  });
  if (body != null) req.write(body);
  req.end();
}

try {
  main();
} catch (e) {
  process.stdout.write(JSON.stringify({ status: 0, error: String((e && e.message) || e) }));
}
