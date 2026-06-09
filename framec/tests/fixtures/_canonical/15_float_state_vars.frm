@@system FloatStateVars {
    interface:
        tick()
        peek(): f32

    machine:
        $Counting {
            $.a: f32 = 0.0
            $.b: f32 = 1.0
            $.c: f32 = 0.4
            tick() {
            }
            peek(): f32 {
                @@:($.b)
            }
        }
}
