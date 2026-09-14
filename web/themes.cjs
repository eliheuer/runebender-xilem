// Compatibility entry point; quality.cjs owns the browser regression checks.
process.env.RUNEBENDER_DPRS ||= '1';
require('./quality.cjs');
