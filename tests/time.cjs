// D6 / specq Time: one rounded-down unit, including the minute/hour/day boundaries.
const fs = require('node:fs'), vm = require('node:vm'), assert = require('node:assert/strict');
const source = fs.readFileSync('assets/app.js', 'utf8');
const age = source.match(/^function time\(s\) \{[\s\S]*?^\}/m);
assert(age, 'shared browser age formatter');
const time = vm.runInNewContext(age[0] + '; time');
for (const [seconds, expected] of JSON.parse(fs.readFileSync('fixture/time.json', 'utf8'))) {
  assert.equal(time(seconds), expected, seconds + ' seconds');
}
assert.equal(time(-1), '<1m');
for (const missing of [null, undefined, NaN, Infinity]) assert.equal(time(missing), '? unavailable');

// Viewer-local exact time: fix the viewer to UTC, without parsing any display form back.
const exact = source.match(/^function exactTime\(t\) \{[\s\S]*?^\}/m);
const exactTime = vm.runInNewContext(exact[0] + '; exactTime', {Intl: {DateTimeFormat: function(locale, options) {
  return new Intl.DateTimeFormat(locale, {...options, timeZone:'UTC'}); }}});
const year = new Date().getFullYear();
const stamp = Date.UTC(year, 10, 5, 14, 30) / 1000;
const weekday = new Intl.DateTimeFormat('en-US', {weekday:'short',timeZone:'UTC'}).format(new Date(stamp * 1000));
assert.equal(exactTime(stamp), `${weekday} Nov 05, 14:30`);
assert.match(exactTime(Date.UTC(year-1, 10, 5, 14, 30) / 1000), new RegExp(`Nov 05, ${year-1}, 14:30$`));
for (const missing of [null, undefined, NaN, Infinity]) assert.equal(exactTime(missing), '? unavailable');
console.log('PASS time boundaries, unknown ages and viewer-local exact times');
