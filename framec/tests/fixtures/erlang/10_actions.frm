@@system Actions {
    interface:
        increment(n: integer)
        get_total(): integer

    machine:
        $Counting {
            increment(n: integer) {
                self._scale(n)
            }
            get_total(): integer { @@:(@@:self.total) }
        }

    actions:
        _scale(n: integer) {
            self.total = self.total + n * 2
        }

    domain:
        total: integer = 0
}
