import Foundation

public struct Vec2: Codable {
    public var x: Double = 0.0
    public var y: Double = 0.0
    public init() {}
    public init(_ x: Double, _ y: Double) { self.x = x; self.y = y }
    public func magSq() -> Double { return x * x + y * y }
}

@@[persist(string)]
@@[save(save_state)]
@@[load(restore_state)]
@@system Bag {
    interface:
        setv(x: Double, y: Double)
        getx(): Double

    machine:
        $S {
            setv(x: Double, y: Double) { @@:self.v = Vec2(x, y) }
            getx(): Double { @@:(@@:self.v.x) }
        }

    domain:
        v: Vec2 = Vec2(0.0, 0.0)
}
