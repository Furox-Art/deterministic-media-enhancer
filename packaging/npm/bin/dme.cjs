#!/usr/bin/env node
'use strict';
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const result = spawnSync('python3', [path.join(__dirname, '..', 'dme_package', 'launcher.py'), ...process.argv.slice(2)], {stdio:'inherit', shell:false});
if (result.error) {
  console.error(JSON.stringify({status:'error',error:result.error.message}));
  process.exitCode = 1;
} else {
  process.exitCode = result.status ?? 1;
}
