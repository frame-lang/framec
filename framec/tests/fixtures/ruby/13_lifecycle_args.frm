@@system LifecycleArgs {
    interface:
        load(n: Integer, label: String)
        total(): Integer
        tag(): String

    machine:
        $Idle {
            load(n: Integer, label: String) {
                -> (n, label) $Active
            }
        }

        $Active {
            $>(count: Integer, name: String) {
                @@:self.sum = count + 1;
                @@:self.label = name;
            }
            total(): Integer {
                @@:(@@:self.sum)
                return
            }
            tag(): String {
                @@:(@@:self.label)
                return
            }
        }

    domain:
        sum: Integer = 0
        label: String = ""
}
