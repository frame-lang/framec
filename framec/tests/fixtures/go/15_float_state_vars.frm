package main

@@system FloatStateVars {
    interface:
        tick()
        peek(): float64

    machine:
        $Counting {
            $.a: float64 = 0.0
            $.b: float64 = 1.0
            $.c: float64 = 0.4
            tick() {
            }
            peek(): float64 {
                @@:($.b)
            }
        }
}