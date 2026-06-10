@@system Actions {
    interface:
        increment(n: number)
        get_total(): number

    machine:
        $Counting {
            increment(n: number) {
                @@:self._scale(n)
            }
            get_total(): number { @@:(@@:self.total) }
        }

    actions:
        _scale(n: number) {
            @@:self.total = @@:self.total + n * 2
        }

    domain:
        total: number = 0
}
