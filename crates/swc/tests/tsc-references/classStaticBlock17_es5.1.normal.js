function _checkPrivateRedeclaration(obj, privateCollection) {
    if (privateCollection.has(obj)) {
        throw new TypeError("Cannot initialize the same private elements twice on an object");
    }
}
function _classApplyDescriptorGet(receiver, descriptor) {
    if (descriptor.get) {
        return descriptor.get.call(receiver);
    }
    return descriptor.value;
}
function _classApplyDescriptorSet(receiver, descriptor, value) {
    if (descriptor.set) {
        descriptor.set.call(receiver, value);
    } else {
        if (!descriptor.writable) {
            throw new TypeError("attempted to set read only private field");
        }
        descriptor.value = value;
    }
}
function _classCallCheck(instance, Constructor) {
    if (!(instance instanceof Constructor)) {
        throw new TypeError("Cannot call a class as a function");
    }
}
function _classExtractFieldDescriptor(receiver, privateMap, action) {
    if (!privateMap.has(receiver)) {
        throw new TypeError("attempted to " + action + " private field on non-instance");
    }
    return privateMap.get(receiver);
}
function _classPrivateFieldGet(receiver, privateMap) {
    var descriptor = _classExtractFieldDescriptor(receiver, privateMap, "get");
    return _classApplyDescriptorGet(receiver, descriptor);
}
function _classPrivateFieldInit(obj, privateMap, value) {
    _checkPrivateRedeclaration(obj, privateMap);
    privateMap.set(obj, value);
}
function _classPrivateFieldSet(receiver, privateMap, value) {
    var descriptor = _classExtractFieldDescriptor(receiver, privateMap, "set");
    _classApplyDescriptorSet(receiver, descriptor, value);
    return value;
}
function _defineProperties(target, props) {
    for(var i = 0; i < props.length; i++){
        var descriptor = props[i];
        descriptor.enumerable = descriptor.enumerable || false;
        descriptor.configurable = true;
        if ("value" in descriptor) descriptor.writable = true;
        Object.defineProperty(target, descriptor.key, descriptor);
    }
}
function _createClass(Constructor, protoProps, staticProps) {
    if (protoProps) _defineProperties(Constructor.prototype, protoProps);
    if (staticProps) _defineProperties(Constructor, staticProps);
    return Constructor;
}
// @target: es2015
var friendA;
var _x = new WeakMap();
var A = /*#__PURE__*/ function() {
    "use strict";
    function A(v) {
        _classCallCheck(this, A);
        _classPrivateFieldInit(this, _x, {
            writable: true,
            value: void 0
        });
        _classPrivateFieldSet(this, _x, v);
    }
    _createClass(A, [
        {
            key: "getX",
            value: function getX() {
                return _classPrivateFieldGet(this, _x);
            }
        }
    ]);
    return A;
}();
var __ = {
    writable: true,
    value: function() {
        friendA = {
            getX: function getX(obj) {
                return _classPrivateFieldGet(obj, _x);
            },
            setX: function setX(obj, value) {
                _classPrivateFieldSet(obj, _x, value);
            }
        };
    }()
};
var B = function B(a1) {
    "use strict";
    _classCallCheck(this, B);
    var x = friendA.getX(a1); // ok
    friendA.setX(a1, x + 1); // ok
};
var a = new A(41);
var b = new B(a);
a.getX();
