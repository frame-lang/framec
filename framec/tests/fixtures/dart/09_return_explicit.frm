@@system ReturnExplicit {
    interface:
        decide(score: int): String

    machine:
        $Judging {
            decide(score: int): String {
                if score >= 60 {
                    @@:return("pass")
                }
                @@:return("fail")
            }
        }
}
