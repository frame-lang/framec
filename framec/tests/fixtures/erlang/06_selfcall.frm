@@system SelfCall {
    interface:
        kick()
        report(): integer

    machine:
        $Active {
            kick() {
                self.count = self.count + 1
                @@:self.report()
            }
            report(): integer { @@:(self.count) }
        }

    domain:
        count: integer = 0
}
