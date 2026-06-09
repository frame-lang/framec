@@system FloatStateVars {
    interface:
        tick()
        peek(): float

    machine:
        $Counting {
            $.a: float = 0.0
            $.b: float = 1.0
            $.c: float = 0.4
            tick() {
            }
            peek(): float {
                @@:($.b)
            }
        }
}
