@@system ReturnExplicit {
    interface:
        decide(score: int): string

    machine:
        $Judging {
            decide(score: int): string {
                if score >= 60 {
                    @@:return("pass")
                }
                @@:return("fail")
            }
        }
}
