@@system FloatStateVars {
    interface:
        tick()
        peek(): double

    machine:
        $Counting {
            $.a: double = 0.0
            $.b: double = 1.0
            $.c: double = 0.4
            tick() {
            }
            peek(): double {
                @@:($.b)
            }
        }
}
