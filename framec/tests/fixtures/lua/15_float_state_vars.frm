@@system FloatStateVars {
    interface:
        tick()
        peek(): number

    machine:
        $Counting {
            $.a: number = 0.0
            $.b: number = 1.0
            $.c: number = 0.4
            tick() {
            }
            peek(): number {
                @@:($.b)
            }
        }
}
