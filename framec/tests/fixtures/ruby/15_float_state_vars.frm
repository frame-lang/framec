@@system FloatStateVars {
    interface:
        tick()
        peek(): Float

    machine:
        $Counting {
            $.a: Float = 0.0
            $.b: Float = 1.0
            $.c: Float = 0.4
            tick() {
            }
            peek(): Float {
                @@:($.b)
            }
        }
}
