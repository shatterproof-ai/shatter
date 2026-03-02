// CommonJS: module.exports object with named functions
function helperA(x) { return x + 1; }
function helperB(x, y) { return x + y; }
module.exports = { helperA, helperB };

// CommonJS: exports.name = function
exports.standalone = function(x) { return x * 2; };
